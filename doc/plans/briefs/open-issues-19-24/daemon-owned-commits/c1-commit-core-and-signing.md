# C1: commit core, signing, merge signing, signing probe

Stage `daemon-owned-commits`, wave 1, tier opus. Read `../common.md` first (Decisions 1-3 and 9 of
`doc/plans/PLAN-open-issues-19-24.md`). Line numbers were read at `ff3fe947`; the stage runs after
`platform-portability` and `session-auth-and-stalls` merge, so locate every edit by symbol.

## Role and issue

Issue #22: a signing operator cannot run unattended, because `~/.gnupg` is a mandatory read denial
in every session sandbox and the session is the only party told to commit. The daemon will commit
for the session, outside the sandbox. You write the git side: the plumbing commit, the shared signing
helper, the signing of loom's own merge commits, the daemon-child capture of the signing environment,
and the startup probe. C2 writes the relay kind and the daemon handler that call you.

## Files owned and files to read

Own exactly (repository-relative): `loom/src/git/stage_commit.rs`, `loom/src/git/stage_commit/tests.rs`,
`loom/src/git/signing.rs`, `loom/src/git/signing/tests.rs`, `loom/src/git/mod.rs`,
`loom/src/git/merge/tree.rs`, `loom/src/daemon/server/environment.rs`,
`loom/src/daemon/server/launch.rs`, `loom/src/commands/run/daemon_child.rs`,
`loom/src/commands/run/foreground.rs`, `loom/src/commands/run/mod.rs`, `loom/src/commands/run/signing_preflight.rs`,
`loom/src/git/runner.rs`, `loom/src/git/stage_commit/checks.rs` (only if `stage_commit.rs` would pass
400 lines).

Read first: `loom/src/git/runner.rs:26-31` (`NO_HOOKS_ARGS`), `:44-53` (`git_timeout`), `:67-83`
(`git_command`, private), `:134-155` (`run_git`, `run_git_with_env`, `run_git_checked`);
`loom/src/git/worktree/pinned.rs:57-62,70-84,104-115` (`discovered`, `pinned`, `run`);
`loom/src/git/merge/tree.rs:196-211` (`commit_merge`), `:240-252` (`PendingMerge::commit`);
`loom/src/git/merge/lock.rs:25` (`MergeLock::acquire`); `loom/src/fs/plan_lifecycle/commit.rs:99-117`
(`attest_plan_commit`, the attestation pattern); `loom/src/daemon/server/environment.rs:5-62`
(`HOST_ENV_ALLOWLIST`, `DaemonEnvironment`); `loom/src/commands/run/mod.rs:117-126`
(`run_startup_preflights`); `loom/src/process/environment.rs:14` (`STAGE_HOST_ENV_ALLOWLIST`).
`loom/src/daemon/server/launch.rs` and `loom/src/commands/run/daemon_child.rs` do not exist at
`ff3fe947`; `platform-portability` creates them (plan section 1, W1): `launch::spawn_daemon(`
re-executes `<loom> run --daemon-child <ABS_WORK_ROOT>` with `env_clear()` plus the daemon allowlist,
and `daemon_child::execute(` is the child entry (setsid, flock). Read both as merged before editing.
Also read the retired design under `doc/plans/briefs/sandbox-escape-hardening/commit-relay/w1-relay-commit-apply.md`
section 5 for the refusal list; it forced `commit.gpgsign=false`, this design signs.

## Pinned interfaces

Provided (quote of `../common.md`, "Commit core" and "Signing"): `CommitRequest { message, expected_head,
expected_tree }`, `CommitScope { StageBranch { stage_id }, Knowledge { target_branch, prefix }, Merge {
stage_id } }`, `CommitRefusal` (Display, Debug), `commit_staged(repo: &Path, scope: &CommitScope, request:
&CommitRequest) -> Result<String, CommitRefusal>`; `SigningEnv { gnupghome: Option<OsString>,
ssh_auth_sock: Option<OsString> }` with `capture_from_process()`, `install(env)`, `installed()`,
`signing_enabled(repo) -> anyhow::Result<bool>`, `probe(repo, env) -> anyhow::Result<()>`.
`commit_merge(repo, tree, [p1, p2], message) -> Result<String>` keeps its signature.

Also provided, for C4 and U1 (they call these in parallel with you):

```rust
// git/signing.rs
pub const SIGN_TIMEOUT: Duration = Duration::from_secs(30);
impl SigningEnv {
    /// GNUPGHOME and SSH_AUTH_SOCK, each only when set.
    pub fn env_pairs(&self) -> Vec<(&'static str, &OsStr)>;
}
// git/stage_commit.rs
pub fn validate_commit_message(message: &str) -> Result<(), String>;
// daemon/server/environment.rs
pub(crate) fn daemon_environment_pairs() -> Vec<(OsString, OsString)>;
```

C4 passes `signing::installed().env_pairs()` and `SIGN_TIMEOUT` to `run_git_with_env_within` for the
plan-completion commit, and matches `CommitTreeError` in the merge landing. U1 calls
`validate_commit_message` and keeps no validator of its own.

Added by this brief, and mirrored in C2's brief (C2 calls them):

```rust
#[derive(Debug)]
pub enum CommitRefusal { Signing { detail: String }, Refused { reason: String } }
// Display: Signing => "signing failed: <detail>", Refused => "<reason>"

pub struct Committer<'a> { /* git: &'a WorktreeGit, repo_root: &'a Path */ }
impl<'a> Committer<'a> {
    pub fn new(git: &'a WorktreeGit, repo_root: &'a Path) -> Self;
    pub fn commit_staged(&self, scope: &CommitScope, request: &CommitRequest)
        -> Result<String, CommitRefusal>;
}
// commit_staged(repo, ..) = Committer::new(&WorktreeGit::discovered(repo), repo).commit_staged(..)
```

Test helpers C2 imports (in `signing/tests.rs`, declared `#[cfg(test)] pub(crate) mod tests;` in
`signing.rs`):

```rust
/// Repo-local commit.gpgsign=true, gpg.format=openpgp and gpg.program=<script>. The script reads its
/// stdin to EOF first (git writes the commit payload to it), then `fail` makes it exit 1 with
/// "fake signer refused" on stderr; otherwise it prints "\n[GNUPG:] SIG_CREATED D 1 8 00 0 FAKE\n" on
/// stderr and an ASCII-armored signature block on stdout. Returns the argv log (one line per call:
/// argv, then GNUPGHOME=<value>).
pub(crate) fn fake_signer(repo: &Path, fail: bool) -> PathBuf;
/// `git` in `dir` with GIT_CONFIG_GLOBAL/GIT_CONFIG_SYSTEM at missing files, GIT_CONFIG_NOSYSTEM=1.
pub(crate) fn git_in(dir: &Path, args: &[&str]) -> String;
```

Why `Committer` and the deviation: the daemon must run index and ref reads through a pinned
`WorktreeGit` (a session can rewrite its worktree `.git` file), and `commit_staged(repo, ..)` cannot
take one. The plan's wiring check wants the literal `commit_staged(` in the handler; the method call
`committer.commit_staged(` satisfies it.

## Root cause and current behaviour

- Every session commit is the session's own `git commit` inside its sandbox; there is no daemon commit path.
- `commit_merge` (`tree.rs:197-211`) runs `commit-tree` without `-S`: loom's merge commits are unsigned
  whatever `commit.gpgsign` says.
- `daemon/server/environment.rs:5-46` has no `GNUPGHOME` or `SSH_AUTH_SOCK`; the daemon never sees them.
- `run_startup_preflights` (`commands/run/mod.rs:117`) checks no signing.
- The runner (`runner.rs:26-31`) already adds `core.hooksPath=/dev/null` and `core.fsmonitor=false` to
  every call, so plumbing run through it never executes a repository hook.

## Tasks

1. **`git/signing.rs`** (`pub mod signing;` and `pub mod stage_commit;` in `git/mod.rs`).
   - `SigningEnv` (Debug, Clone, Default), `capture_from_process()` reads the two variables with
     `std::env::var_os`; `install` stores into a `OnceLock<SigningEnv>` and ignores a second call;
     `installed()` returns the stored value or a `static` default.
   - `signing_enabled(repo)`: `run_git(["config", "--type=bool", "commit.gpgsign"], repo)`; exit 0 with
     stdout `true` is `Ok(true)`, `false` is `Ok(false)`, exit 1 (unset) is `Ok(false)`, any other exit is
     `Err` carrying the stderr (a malformed value must not silently unsign).
   - `pub struct CommitTreeError { pub signing: bool, pub detail: String }` (Display, Error). `pub fn
     commit_tree(repo, tree, parents: &[&str], message) -> Result<String, CommitTreeError>` = `signing_enabled`
     then `commit_tree_with(.., installed())`. `commit_tree_with(.., env: &SigningEnv)` runs
     `commit-tree <tree> -p <parent>... [-S] -m <message>`. Unsigned goes through `run_git_checked`
     (identical to today). Signed goes through a new runner function in `git/runner.rs` (381 lines;
     stay under 400), `pub(crate) fn run_git_with_env_within(repo: &Path, args: &[&str], env: &[(&str,
     &OsStr)], timeout: Duration) -> Result<Output>`, built on the private `git_command` so it keeps
     every hardening flag, with the env pairs set on that one `Command`; `commit_tree_with` passes
     `&env.env_pairs()` and `SIGN_TIMEOUT`. The sketch below shows what the call must amount to; do
     not duplicate the hardening outside `runner.rs`:

   ```rust
   fn signing_command(repo: &Path, args: &[&str], env: &SigningEnv) -> Command {
       let mut command = Command::new("git");
       command.args(NO_HOOKS_ARGS).args(["-c", "core.commitGraph=false"]).args(args)
           .env("LC_ALL", "C").env("LANG", "C").env("GIT_NO_REPLACE_OBJECTS", "1")
           .env("GIT_GRAFT_FILE", "/dev/null/loom-no-grafts").current_dir(repo);
       for (name, value) in env.env_pairs() { command.env(name, value); }
       command
   }
   ```

   Inside `run_git_with_env_within`, run it with `crate::process::run_bounded_output(&mut command, timeout, "git commit-tree -S")`.
   `signing.rs` exports `pub const SIGN_TIMEOUT: Duration` (30 s) for every signing call, and
   `impl SigningEnv { pub fn env_pairs(&self) -> Vec<(&'static str, &OsStr)> }` returns `GNUPGHOME` and
   `SSH_AUTH_SOCK` for the fields that are set. A non-zero exit, a spawn failure or a `ProcessTimeoutError` is `CommitTreeError { signing: true,
   detail: <last 8 stderr lines, at most 1,000 bytes> }`; a timeout names "30 s" and says the signing agent
   probably needs a cached passphrase. The two variables are set on this one `Command` only.

   - `probe(repo, env)`: `hash-object -w -t tree /dev/null` through `run_git_checked` gives the empty
     tree; then the signed `commit-tree` with no parent and message `loom signing probe`. Error text:
     `commit.gpgsign is true but a test signature failed: <detail>. Loom signs every stage and merge
     commit as the daemon, without a terminal. Cache the passphrase in gpg-agent or use an SSH key held
     by ssh-agent, check it with git commit-tree -S, then run loom run again.`

2. **`commit_merge`** in `git/merge/tree.rs`: body becomes `signing::commit_tree(repo, tree,
   &[parents[0], parents[1]], message).map_err(anyhow::Error::from)`; imports adjust; nothing else moves.
   The `CommitTreeError` must stay downcastable anywhere in the anyhow chain: `anyhow::Error::from`, never a
   string conversion (`to_string`, `anyhow!("{e}")`). C4 matches it in the merge landing with
   `error.chain().find_map(|e| e.downcast_ref::<CommitTreeError>())`, and `PendingMerge::commit` already
   passes the error through with `?`.
3. **`git/stage_commit.rs`** (at most 400 lines; each function under 50; if the checks do not fit, add
   `git/stage_commit/checks.rs` and report it). `Committer::commit_staged`, in order, each a refusal
   `CommitRefusal::Refused { reason }` naming what it saw:
   1. Message: `validate_commit_message(&request.message)`, a `pub fn validate_commit_message(message:
      &str) -> Result<(), String>` in `stage_commit.rs`: non-empty after `trim`, at most 16 KiB, no NUL, and
      no AI attribution. The attribution rules live only here (U1 calls this function and keeps no copy; a
      session can write a relay ticket without the CLI, `relay/emit.rs`, so the daemon applies them too):
      per line, lowercased, refuse a line starting with
      `co-authored-by:` or `signed-off-by:` that contains `claude` or `anthropic`; any text containing
      `noreply@anthropic`; the words `generated with` together with `claude code`, `claude.ai` or
      `claude.com`. Plain prose that merely says claude, with no trailer, passes. `expected_head` and
      `expected_tree` must be 40 or 64 lowercase hex characters (they reach git argv; never pass an
      unchecked agent string).
   2. Branch: scope branch is `refs/heads/loom/<stage_id>` (StageBranch, Merge; validate `stage_id` with
      `crate::validation::validate_id`) or `refs/heads/<target_branch>` (Knowledge; refuse a name starting
      with `-`). `git.run(["symbolic-ref", "--quiet", "HEAD"])` must print exactly it; exit 1 is a detached HEAD.
   3. `rev-parse --verify HEAD` equals `expected_head`.
   4. `MERGE_HEAD`: `rev-parse --verify --quiet MERGE_HEAD`. Present outside a Merge scope refuses; a Merge
      scope without it refuses.
   5. `ls-files --unmerged -z` must be empty.
   6. `write-tree` equals `expected_tree`. From here use that tree id, never the index again.
   7. `diff-tree -r -z --raw --no-renames --ignore-submodules=none <head> <tree>` (trees, not the index, so
      nothing can change under you). Refuse: a new mode of `160000`; a path whose first component is
      `.loom`, `.work` or `.worktrees`, compared with `eq_ignore_ascii_case` (`.LOOM/work/x` is refused like
      `.loom/work/x`: a case-insensitive filesystem resolves both names to one directory, as
      `is_control_path` in `git/merge/control_paths.rs` already does); for Knowledge a path not under
      `prefix` (compare with `Path::starts_with`); an empty diff outside a Merge scope ("nothing to
      commit"). Merge scope exception: a merge legitimately carries the target's own gitlinks and state
      paths, so there a gitlink or state path is refused only when its entry in the new tree differs from
      BOTH HEAD's tree and MERGE_HEAD's tree. Run the same `diff-tree` against `<merge_head> <tree>` and
      refuse a flagged path only when it is flagged in both diffs, so a submodule bump made on the target
      still merges.
   8. `signing::commit_tree` at `self.repo_root` (objects are shared; the config is the main repository's)
      with parents `[head]` or `[head, merge_head]`. `CommitTreeError { signing: true }` becomes
      `CommitRefusal::Signing`. The ref is unmoved.
   9. `update-ref -m "loom: commit <first message line>" <ref> <new> <head>` through `self.git.run` (the
      old value is the compare-and-swap). A failure refuses with git's stderr; the ref is unmoved.
   10. Merge scope only: `git.run(["merge", "--quit"])`; a failure is `tracing::warn!`, not a refusal (the
      ref already moved). Return the new id.
   - Attestation and the merge lock are NOT here: `commit_staged` has no `work_dir`. C2's handler takes the
     lock around Knowledge commits and calls `target_guard::append_attestation` when the moved ref is
     guarded (as `attest_plan_commit` does at `plan_lifecycle/commit.rs:99-117`).
4. **Signing environment.** In `daemon/server/environment.rs` add `const SIGNING_ENV_NAMES: [&str; 2] =
   ["GNUPGHOME", "SSH_AUTH_SOCK"]` and let `is_allowed` accept them, so the parent forwards them to the
   daemon child only. Add `XDG_CONFIG_HOME` and `GIT_CONFIG_GLOBAL` to `HOST_ENV_ALLOWLIST`: they are
   locations, not credentials, and without them the daemon can read a different global git config than the
   operator (and so a different `commit.gpgsign`). Export `pub(crate) fn daemon_environment_pairs() ->
   Vec<(OsString, OsString)>`: the allowlisted subset of the current process environment, exactly what the
   daemon child receives (`DaemonEnvironment::capture_from(std::env::vars_os())`'s variables); task 6 uses
   it. If `launch::spawn_daemon(` builds the child environment from `DaemonEnvironment`, nothing in
   `launch.rs` changes; if it builds its own list, add the four names there and nowhere else. None of them
   may join `STAGE_HOST_ENV_ALLOWLIST` (`process/environment.rs`, not yours).
5. **`commands/run/daemon_child.rs`**: as the first statements of `execute(`, before the tokio runtime or
   any thread: `let env = SigningEnv::capture_from_process(); signing::install(env);` then remove both
   variables with `std::env::remove_var`. In this edition (2021, `loom/Cargo.toml`) it is a safe function,
   and an `unsafe { .. }` block around it compiles too, as the `set_var` block at `commands/run/mod.rs:57-59`
   at `ff3fe947` did (`platform-portability` removes it). Either form is fine; the rule that matters is to
   call it before any thread exists, so say that in a comment.
6. **`commands/run/signing_preflight.rs`**: when `commit.gpgsign` is true the daemon must be able to sign
   in the environment it will actually have, not the operator's. Two functions:
   - `pub(super) fn require_signing(repo_root: &Path) -> Result<()>` (production wrapper): builds the
     operator pairs from `std::env::vars_os()`, adding `GPG_TTY` (from `nix::unistd::ttyname` on stdin,
     the `term` feature is on) only when stdin is a terminal and `GPG_TTY` is unset, so a terminal pinentry
     can prompt and the agent caches the passphrase; never `set_var`, never on the daemon pairs. It builds
     the daemon pairs from `daemon_environment_pairs()` plus `SigningEnv::capture_from_process().env_pairs()`
     and calls `require_signing_with`.
   - `fn require_signing_with(repo_root: &Path, operator: &[(OsString, OsString)], daemon: &[(OsString,
     OsString)]) -> Result<()>`, split out so a test passes both environments explicitly and mutates no
     process variable. Each run is a complete environment (`env_clear()` plus the given pairs) built on
     `runner.rs`'s hardened `git_command`; add the small helpers you need to `git/signing.rs` and
     `git/runner.rs` (settle the names, report them) and never duplicate the hardening flags elsewhere.
     Steps: (1) read `commit.gpgsign` under `operator`; `Ok` when it is not true; else probe a signature
     under `operator`. (2) Read `commit.gpgsign` again under `daemon` and probe again under `daemon`
     (empty-tree `commit-tree -S`, `SIGN_TIMEOUT`); refuse when this run reads a different `commit.gpgsign`
     (name the cause: configuration the daemon's allowlisted environment cannot see, such as
     `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_0`/`GIT_CONFIG_VALUE_0`, and how to move it into a config file) or
     fails to sign (the existing `probe` error text). (3) After both pass, read `gpg.format`; when it is
     `openpgp` or unset print once:
     `gpg-agent forgets a cached passphrase 600 s after its last use (default-cache-ttl; at most
     max-cache-ttl, 7200 s): raise both in gpg-agent.conf or sign with an ssh-agent key, or stages block at
     their first commit.` Keep the format decision in a small pure fn so it is testable.
   In `run_startup_preflights`, after `sandbox_preflight::require_sandbox_prerequisites` and before
   `advisory_codex_lane_preflight`, call
   `signing_preflight::require_signing(work_dir.repo_root().context("cannot resolve the repository root")?)?;`
   (keep `session-auth-and-stalls`' `auth_preflight` call where that stage put it).
   `loom run --foreground` shares the function, so it runs both probes as well. Its process runs the
   orchestrator itself, so it captures too: see task 7. Report it if the stricter second run refuses a
   foreground run the operator environment would sign.
7. **Foreground capture (plan Decision 9).** `signing.rs` exports `pub fn take_from_process()`
   (`SigningEnv::capture_from_process()`, `install`, then `std::env::remove_var` of `GNUPGHOME` and
   `SSH_AUTH_SOCK`) and `pub fn current() -> SigningEnv` (a clone of the installed `SigningEnv` when
   `take_from_process` ran, read through the `OnceLock`'s `get()`, else
   `SigningEnv::capture_from_process()`). Task 5's three statements become the one call
   `signing::take_from_process();`. `foreground::execute` (`commands/run/foreground.rs`, yours)
   makes the same call as its first statement: `main.rs` spawns no thread before dispatch, and the
   comment says so. `require_signing` adds `current().env_pairs()` to both probe sources, so the
   foreground run probes exactly as the background parent does, although its own environment no
   longer carries the two variables:
   - operator pairs: `std::env::vars_os()` plus `current().env_pairs()`;
   - daemon pairs: `daemon_environment_pairs()` plus `current().env_pairs()`.

   Keep the choice in a pure `current_from(installed: Option<&SigningEnv>, capture: impl FnOnce() ->
   SigningEnv) -> SigningEnv`. Test it as
   `git::signing::tests::the_installed_signing_environment_wins_over_the_process` (exact name;
   acceptance runs it `--exact`). No test calls `take_from_process` or `install`, because both
   change process-wide state.

## Tests to write

`git::signing::tests` (file `git/signing/tests.rs`): `signing_enabled_reads_the_bool_forms`,
`signing_enabled_rejects_a_malformed_value`, `commit_tree_is_unsigned_when_gpgsign_is_off`,
`commit_tree_signs_once_and_the_commit_has_a_gpgsig_header`, `signing_environment_reaches_only_the_signing_call`
(the log shows `GNUPGHOME=/x` for a `SigningEnv` carrying it; for the default `SigningEnv` the signer
inherits the test process's own `GNUPGHOME`, so assert the logged value equals
`std::env::var_os("GNUPGHOME")` (an unset variable logs as empty), never an unconditionally empty value;
and `std::env::var_os("GNUPGHOME")` is untouched), `a_failing_signer_is_a_signing_error_with_its_stderr_tail`,
`probe_passes_with_a_working_signer`, `probe_names_the_signer_failure`, and
`merge_commit_is_signed_through_commit_merge` (two parents, `gpgsig` present).
`git::stage_commit::tests`: `commits_the_staged_tree_with_head_as_parent`, `signs_when_gpgsign_is_true`,
`signing_failure_leaves_the_ref_unmoved`, `refuses_a_moved_head`, `refuses_a_tree_mismatch`,
`refuses_the_wrong_branch`, `refuses_a_detached_head`, `refuses_a_gitlink`, `refuses_a_staged_state_path`,
`refuses_a_staged_state_path_in_another_case` (`.LOOM/work/x`), `merge_scope_accepts_the_targets_gitlink`
(MERGE_HEAD's tree and the new tree carry a gitlink HEAD's tree lacks; the commit succeeds; a gitlink
present in neither parent tree is still refused in a Merge scope),
`refuses_merge_head_outside_a_merge_scope`, `merge_scope_takes_merge_head_as_second_parent_and_quits`,
`merge_scope_without_merge_head_refuses`, `knowledge_scope_refuses_a_path_outside_the_prefix`,
`knowledge_scope_commits_under_the_prefix`, `refuses_an_empty_commit`, `refuses_a_bad_message_or_object_id`,
`planted_hooks_never_run` (executable pre-commit, commit-msg, post-commit, reference-transaction hooks that
create marker files), `a_stale_old_value_fails_the_update_ref`.
In `daemon/server/environment.rs` (existing `tests` module, new function): `signing_variables_reach_the_daemon_child_but_never_a_stage_environment`:
`DaemonEnvironment::capture_from` keeps both (and keeps `XDG_CONFIG_HOME` and `GIT_CONFIG_GLOBAL`, which
`daemon_environment_pairs()` also returns: assert every name it returns passes `is_allowed`); `crate::process::apply_stage_environment_from` on a
`Command` with `HOME`, `GNUPGHOME`, `SSH_AUTH_SOCK` leaves only `HOME` in `get_envs()`, and
`crate::process::agent_session_environment_from` drops both (`session-auth-and-stalls` made both public).
In `commands/run/signing_preflight.rs` (inline `mod tests`, module path
`commands::run::signing_preflight::tests`): `require_signing_passes_when_signing_is_off`,
`require_signing_names_the_signer_failure`, and `the_daemon_environment_reads_the_same_gpgsign_as_the_operator`
(exact name; acceptance runs it `--exact`): a repository whose repo-local config has a working fake signer
(`fake_signer(repo, false)`, then unset its repo-local `commit.gpgsign` so only the operator source enables
it); call `require_signing_with` with an `operator` source that enables gpgsign only through
`GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=commit.gpgsign`, `GIT_CONFIG_VALUE_0=true` and a `daemon` source
that carries neither (the allowlist drops them); both sources carry `HOME`, `PATH` and the three
`GIT_CONFIG_*` isolation variables. The result is an `Err` naming the different `commit.gpgsign`. A second
case passes the same `operator` pairs as `daemon` and gets `Ok`. A third case with a failing signer is an
`Err` naming the signer failure. Add a fourth, pure test of the format decision
(`the_cache_caveat_applies_to_openpgp_and_unset_formats`).

## Patterns to copy and not copy

Copy: the runner's hardening and exit-code handling (`runner.rs:67-83`, `:157-188`); the fixture style of
`loom/src/verify/impact_tests_tests.rs` (git with the three `GIT_CONFIG_*` variables on the fixture's own calls);
`attest_plan_commit` for the guard attestation shape (C2 applies it). Do NOT copy `commit_rename`
(`commands/run/plan_inputs.rs:107-137`): it is porcelain `git commit`, which reads the worktree and runs
the operator's hooks; the daemon never does either. In every test set `commit.gpgsign` and `gpg.program`
repo-locally; never rely on the host's global configuration.

## Traps

- Knowledge: "Loom never runs a three-way merge in the operator's main checkout ... `merge_stage` computes the
  merge with `merge-tree` and `commit-tree`" (`conventions/git-and-build-workflow.md`, Git Operations); your
  signed `commit_merge` changes only the `commit-tree` call. A signing failure surfaces as a
  `CommitTreeError { signing: true }` inside the anyhow chain; what the merge landing does with it
  (`NeedsHumanReview` and `Landing::Held`) is C4's, not an "existing path". Keep it downcastable and never
  swallow or stringify it.
- Knowledge: "`merge_stage` ... never checks out a branch, runs a three-way merge, or creates `MERGE_HEAD` in
  the operator's main checkout" (`architecture/merge-flow.md`). The Merge scope runs in a stage worktree
  (`signals/merge.rs` tells the resolver to merge there), never in the operator's checkout.
- Knowledge: the reference-transaction hook "appends `attest <current> <new> <ref>`" and loom's own git runs
  without hooks, so a guarded ref you move is unattested (`architecture/target-guard.md`, Attestation).
  Return the id; C2 attests.
- A session can rewrite `MERGE_HEAD`, the index and the `.git` file of its worktree; hence the pinned
  `WorktreeGit` for index reads, the tree-based path check, and `commit-tree` at the repository root.
- Existing tests that reach `commit_merge` now sign when the host has `commit.gpgsign=true` globally. Report
  any such test you notice; do not edit its assertions.
- `std::env::remove_var` is a safe function in this edition (2021, `loom/Cargo.toml`); an `unsafe` block
  around it compiles as well. The rule that matters is to call it before any thread exists.
- The probe's second run needs the daemon's environment, not the operator's: `GIT_CONFIG_COUNT`,
  `GIT_CONFIG_PARAMETERS` and similar variables are not on the allowlist, so a repository whose signing is
  enabled only through them is refused at `loom run`, not discovered at the first stage commit.

## The one check

`cargo test --lib git::signing::` once, after your files compile together. A compile error in a file you
do not own (C2, C3, C4, U1, U2 write in parallel) is theirs: report it with file and line.

## Report format

Files changed (flag any new sibling module); the check and its result; deviations from this brief and from
`../common.md` with the reason; ledgered units you shrank; any contradiction between the tree and this brief.
