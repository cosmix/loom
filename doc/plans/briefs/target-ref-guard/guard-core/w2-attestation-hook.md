# W2: the reference-transaction hook and its installer

Stage `guard-core`, wave 2 (after W1 returned and the main agent verified it), tier opus. Read
`../common.md` first. W1's `loom::git::target_guard` exists when you start; read
`loom/src/git/target_guard/mod.rs` and `record.rs` for the file formats you share.

You own `loom-hooks/git-reference-transaction-hook.sh` (new),
`loom/src/git/hooks.rs` (the installer re-exports and the `core.hooksPath` reader of section
2b), `loom/src/git/hooks/reference_transaction.rs` (new),
`loom/src/git/hooks/reference_transaction_tests.rs` (new),
`loom/src/commands/init/execute.rs`, `loom/src/commands/init/git_hooks.rs` (new),
`loom/src/commands/init/mod.rs` (one `mod git_hooks;` line),
`loom/tests/target_ref_hook_tests.rs` (new), and `loom/maintainability-baseline.txt` (the
`execute` line only).

Frozen, read-only: `loom/tests/target_ref_hook_contracts.rs`. Read it first.

Your one check, run once at the end, from `loom/`:
`cargo test --test target_ref_hook_contracts && cargo test --test target_guard_contracts && cargo test --test target_ref_hook_tests && cargo test --lib git::hooks && cargo test --lib commands::init::`.

## Why

A git hook that runs in the agent's own git cannot be a gate: the agent can pass
`-c core.hooksPath=/dev/null` or write the ref file directly (plan section 1, measured). It can
be an attestation channel. Git runs `.git/hooks/reference-transaction` for every ref update made
through its refs API (measured: `update-ref`, `branch -f`, `push .`, `fetch .`,
`merge --ff-only`, `commit`, `reset --hard`). The hook appends each move of a guarded target to
`.loom/work/target-guard.ledger`. The operator's git runs on the host and can write it; a
stage agent's git runs in a sandbox that denies `.loom/` as a directory and cannot. A skipped
hook leaves no line, so the daemon holds the move. When a loom session cannot attest, the hook
refuses the update, which stops honest mistakes for every git spelling.

## 1. The hook script `loom-hooks/git-reference-transaction-hook.sh`

POSIX `#!/bin/sh` (no bash-isms: it runs on every ref update in the repository, so it must be
cheap and portable). Its second line is the comment `# LOOM_REFERENCE_TRANSACTION_HOOK` (the
value of `target_guard::HOOK_MARKER`). Behaviour, with `$1` the phase:

1. Phases other than `prepared` and `aborted`: drain stdin (`cat >/dev/null`), exit 0. Then
   `export GIT_NO_REPLACE_OBJECTS=1 GIT_GRAFT_FILE=/dev/null`, so the hook's own git ignores
   replace refs and grafts like loom's runner (plan section 3.1).
2. `common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)`; on failure
   drain and exit 0. `work="$(dirname "$common")/.loom/work"`, `refs="$work/target-guard.refs"`,
   `ledger="$work/target-guard.ledger"`. No `$refs` file: drain and exit 0 (no loom run guards
   this repository).
3. `allow` = the value of the `allow` line of `$refs`. An empty or missing value means no
   knowledge exception (an empty prefix would match every path).
4. For each stdin line `old new ref` whose `ref` appears as a `ref <ref>` line of `$refs`
   (exact match):
   - `cur=$(git rev-parse -q --verify "$ref^{commit}")` (empty when the ref does not exist).
     Git passes the all-zero id as `old` when the caller gave no old value (`update-ref <ref>
     <new>`, `branch -f`), so the hook records `cur`, never `old`. `zero` = `$new` with every
     hex digit replaced by `0` (`sed 's/[0-9a-f]/0/g'`; `tr '0-9a-f' '0'` relies on GNU padding
     the second set), so sha256 repositories work. Skip a line whose `new` starts with `ref:`
     (a symref update).
   - `prepared`: when `( : >> "$ledger" ) 2>/dev/null` succeeds, append
     `attest ${cur:-$zero} $new $ref`. Otherwise, when `LOOM_SESSION_ID` is non-empty and the
     move is not knowledge-only (below), print the refusal to stderr and remember to exit 1.
     Otherwise allow it without a line.
   - `aborted`: when the ledger is writable, append `abort ${cur:-$zero} $new $ref` (after an
     abort the ref still holds `cur`, the value `prepared` recorded).
5. Exit 1 when any line was refused (git then aborts the whole transaction), else 0.

Knowledge-only (all must hold): `cur` is non-empty; `$new` is not the zero id;
`git merge-base --is-ancestor "$cur" "$new"`; every line of
`git -c core.quotePath=true diff --name-only --no-renames "$cur" "$new"` starts with `$allow`
(a quoted path starts with `"` and so fails closed).

Refusal text (stderr, one paragraph):
`loom: refusing to move <ref> from loom session <LOOM_SESSION_ID>: this session cannot record
the move for the operator. Loom merges your stage branch into the target after
'loom stage complete'; a move made outside loom holds every merge until the operator reviews
it.`

Keep the script under 80 lines. No `jq`, no bash arrays, no `[[ ]]`.

## 2. The installer

`loom/src/git/hooks.rs` keeps the pre-commit installer; declare `mod reference_transaction;`
there and re-export `pub use reference_transaction::{install_reference_transaction_hook,
is_reference_transaction_hook_installed, HookInstall};`. In `hooks/reference_transaction.rs`:

- `pub(super) const SCRIPT: &str =
  include_str!("../../../../loom-hooks/git-reference-transaction-hook.sh");` (four levels from
  `src/git/hooks/`, the same depth `fs/permissions/constants.rs` uses).
- Declare the test module from `hooks.rs` (`#[cfg(test)] #[path = "hooks/reference_transaction_tests.rs"]
  mod reference_transaction_tests;`): declared from `reference_transaction.rs` a plain
  `mod reference_transaction_tests;` would resolve to `hooks/reference_transaction/…`.
- `install_reference_transaction_hook(repo_root)`: refuse (`bail!`) when `repo_root/.git` is not
  a directory, as `install_pre_commit_hook` does. Target: `repo_root/.git/hooks/
  reference-transaction` (create `hooks/` if missing). Absent: write `SCRIPT`, mode 0755,
  `Installed`. Present and containing `target_guard::HOOK_MARKER`: equal to `SCRIPT` →
  `UpToDate`; else overwrite → `Installed`. Present without the marker (another tool's hook):
  leave it untouched → `ForeignHookPresent`.
- `is_reference_transaction_hook_installed(repo_root)`: the file exists and contains the marker.
- A unit test asserts `SCRIPT.contains(target_guard::HOOK_MARKER)`.

`hooks.rs` is 256 lines; keep it under 400.

## 2b. Read `core.hooksPath` the way git applies it

`attestation_mode` (W1) and `control_paths::hooks_dir_prefix` decide through
`hooks.rs::read_hooks_path_scope` and `configured_hooks_path`, which miss two places git takes
`core.hooksPath` from (measured on git 2.53):

- `git config --local --get core.hooksPath` exits 1 when the value sits in an `include.path`
  file, while git applies it: a scoped read skips includes unless `--includes` is given. Add
  `--includes` to `read_hooks_path_scope`'s argument list (`["config", scope, "--includes",
  "--get", "core.hooksPath"]`), which fixes both callers.
- With `extensions.worktreeConfig` on, `git config --worktree core.hooksPath x` writes
  `config.worktree`, which a `--local` read does not see. Make `configured_hooks_path` try
  `--worktree` first (`["--worktree", "--local", "--global", "--system"]`, git's precedence).
  Without the extension `--worktree` reads the same file as `--local` in a repository with no
  linked worktree, and exits 128 ("--worktree cannot be used with multiple working trees") in
  one that has them (measured); `read_hooks_path_scope` maps a failed read to `None`, which is
  right there: with the extension off git reads no `config.worktree`. Update both doc comments.

The reads stay scoped: an unscoped read returns the `-c core.hooksPath=/dev/null` loom's runner
forces (command scope). `hooks_dir_prefix` keeps its three scopes (the control-path prefix is a
tracked directory; worktree config does not change which files are tracked).

Tests, in `tests/target_ref_hook_tests.rs`, each in a temp repo with the hook installed:
an `include.path` file setting `core.hooksPath = /dev/null` makes
`loom::git::configured_hooks_path` return `Some("/dev/null")` and
`target_guard::attestation_mode` return `Off`, and a host commit on `main` then writes no ledger
line (git really skips the hook, so `Off` is the truth); `git config extensions.worktreeConfig
true` plus `git config --worktree core.hooksPath /dev/null` gives the same `Off`.

## 3. `loom init` installs it

In `loom/src/commands/init/execute.rs` (364 lines), `execute` (ledger: 130 lines) installs the
pre-commit hook in a 21-line block (about lines 138-158). `execute.rs` has no room for both
installers (one function holding both matches is about 50 lines; two functions take the file
past 400), so move that block into a new file `src/commands/init/git_hooks.rs` (declared
`mod git_hooks;` in `init/mod.rs`): `pub(super) fn install_git_hooks(repo_root: &Path)` calls one
helper per hook. The pre-commit helper behaves as today; the reference-transaction helper prints
in the same style: `Git reference-transaction hook installed` / `already up to date`, and for
`ForeignHookPresent` a yellow warning: `.git/hooks/reference-transaction belongs to another
tool; loom cannot attest operator moves of the target, so the target guard runs without
attestation`. An install error prints the same kind of warning the pre-commit branch prints and
does not fail `init`. `execute` calls `git_hooks::install_git_hooks(&repo_root)` and shrinks:
lower its ledger line to the new exact count (common.md). Add a `#[cfg(test)] mod tests` in
`git_hooks.rs` asserting that `install_git_hooks` on a temp repo writes
`.git/hooks/reference-transaction` containing `HOOK_MARKER` and mode 0755.

## 4. Tests

`loom/tests/target_ref_hook_tests.rs` (integration test file, discovered by cargo): a temp
repo on `main` with one commit, a linked worktree `.worktrees/s` on `loom/s` with one commit,
`.loom/work/` created, the hook installed with `install_reference_transaction_hook`, and the
guard initialised with `loom::git::target_guard::check(root, &work, "main")` (writes the refs
file and the empty ledger). Simulate a sandbox that cannot write the ledger by replacing
`target-guard.ledger` with a directory (that fails for root too, unlike `chmod`). Cases:

- a commit on `loom/s` writes no ledger line (not a guarded ref);
- an aborted transaction: `printf 'start\nupdate refs/heads/main <x>\nprepare\nabort\n' | git
  update-ref --stdin` exits 0, `main` is unchanged, and the ledger holds an `attest` line then a
  matching `abort` line (a wrong old value is NOT a way to get there: git fails it while taking
  the ref lock, before `prepared`, so the hook never runs);
- no `target-guard.refs` file: the hook does nothing, even with an unwritable ledger and
  `LOOM_SESSION_ID` set;
- `git -C .worktrees/s update-ref refs/heads/main <x>` from the worktree, ledger writable, writes
  the line into the main checkout's ledger;
- unwritable ledger, `LOOM_SESSION_ID` unset: the move succeeds and writes no line;
- unwritable ledger, `LOOM_SESSION_ID` set: a commit on `main` in the main checkout adding
  `doc/loom/knowledge/é.md` is refused, because git quotes the non-ASCII name and the quoted
  path does not start with the prefix (fail closed);
- unwritable ledger, `LOOM_SESSION_ID` set, the main checkout detached first (`git checkout
  --detach`; with `main` checked out git itself refuses the push, so the test would pass without
  the hook): `git -C .worktrees/s push . HEAD:main` is refused by the hook and `main` is
  unchanged;
- `-c core.hooksPath=/dev/null` moves the target and writes nothing.

`hooks/reference_transaction_tests.rs`: `Installed`, `UpToDate`, overwrite of an older loom
hook, `ForeignHookPresent` leaves the foreign file byte-identical, missing `.git` errors.

## Traps

- `#!/bin/sh` is dash on Debian/Ubuntu: test with the real script, not a sourced copy.
- A new file lands at mode 0664: the installer sets 0755 on the installed copy; the repository
  copy needs no exec bit (it is embedded).
- Read every stdin line even after a refusal, then exit 1.
- The hook must not print anything on success: git passes hook stdout through to the user.
