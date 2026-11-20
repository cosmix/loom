# W1: the `commit` relay kind and the daemon's commit

Stage `commit-relay`, wave 1, tier opus. Read `../common.md` first (decision D1 is this
stage). Line numbers were read at `3fc28031` (unchanged at `2908339a`); the plan runs after
PLAN-stage-exits-and-environment merges, so locate every edit by symbol.

## Why

Decision D1 puts the whole git common directory in every worktree session's `denyWrite`, so
`git add` and `git commit` fail in a stage worktree with EROFS. The session instead asks the
daemon to commit: `loom commit` (W2) writes a relay ticket of a new control kind `commit`, the
relay hook moves it into `<work_dir>/inbox/<session-id>/` (`<work_dir>` is the state directory
`.loom/work`), and the daemon's inbox drain applies it at most once. `W` in this brief is the
worktree's git administrative directory, as in `common.md`. You build the protocol type, the matrix column, the one path validator both sides share,
and the daemon's apply. This is the security core of the stage: the daemon runs git on the host,
outside every sandbox, on a request the agent wrote.

## Files you own

- `loom/src/relay/kind.rs`, `loom/src/relay/matrix.rs`, `loom/src/relay/tests_matrix.rs`,
  `loom/src/relay/payload.rs`, `loom/src/relay/mod.rs`, `loom/src/relay/commit_paths.rs` (new)
- `loom/src/orchestrator/core/inbox_drain.rs` (module root: one `mod` line each for the new files)
- `loom/src/orchestrator/core/inbox_drain/apply.rs`
- `loom/src/orchestrator/core/inbox_drain/commit.rs`, `commit_read.rs`, `commit_tests.rs` and
  `commit_tests_files.rs` in the same directory (all new)
- `loom/src/orchestrator/core/inbox_drain/test_support.rs`,
  `loom/src/orchestrator/core/inbox_drain/tests_matrix.rs`
- `loom/src/commands/hook/relay/tests.rs`, `loom/src/commands/hook/relay/tests_sweep.rs`

`inbox_drain/sweep.rs`, `inbox_drain/tests_sweep.rs` and the new `inbox_drain/tests_sweep_cache.rs`
belong to stage `capsule-policy`: never touch them. That stage runs in parallel with this one, and
`tests_sweep_cache.rs` imports `fixture` and `payload_for` from your `test_support.rs` (section 7:
additive changes only). `loom/src/git/**` (including `git/worktree/pinned.rs`) belongs to
`host-git-integrity`: use `WorktreeGit` read-only; if you need more from it, write it in your own
file on top of `WorktreeGit::run`.

W2 (CLI, wave 2) calls `CommitRequest` and `check_commit_request`; pin those signatures exactly
as below.

## 1. The kind (`relay/kind.rs`)

- Add `Commit` as the LAST variant of `RequestKind` (wire name `commit`, `wire_name` arm).
- `all()` returns `[RequestKind; 10]` with `Commit` last; update the doc comment ("All ten kinds
  ... then `commit` from sandbox-escape-hardening D1") and the module doc ("The ten request
  kinds").
- `is_control()` includes `Commit`; its doc says "The eight kinds only a session's lead process
  may relay".
- Test `is_control_matches_the_seven_control_kinds`: rename it
  `is_control_matches_the_control_kinds` and add `RequestKind::Commit,` as the last element of
  the expected vec. Add no other change to that assertion. Add
  `commit_is_kebab_case_on_the_wire` beside the other wire-name tests.

Every exhaustive `match` on `RequestKind` or `RequestPayload` in the crate breaks when the variant
lands. At `3fc28031` they are: `relay/kind.rs::wire_name`, `relay/payload.rs::decode_payload`,
`inbox_drain/apply.rs::apply`, `inbox_drain/test_support.rs::payload_for`,
`inbox_drain/tests_matrix.rs::classify_applied`. Confirm with
`rg -n 'RequestKind::FileDispute|RequestPayload::FileDispute' loom/src loom/tests` and report any
other hit (it is in a file you do not own).

## 2. The matrix (`relay/matrix.rs`, `relay/tests_matrix.rs`)

Append six rows to `MATRIX`: `(Stage, Commit, Apply)`, and `Refuse` for Knowledge, Merge,
BaseConflict, Adjudication and Contract. Extend the `MATRIX` doc comment: "`commit`
(sandbox-escape-hardening D1) is a `Stage` session's alone: checkout sessions keep git, and a
contract session never commits." A Contract session is worktree-rooted but its signal says "Do
not commit" (`orchestrator/signals/contract.rs:242`).

In `tests_matrix.rs` leave `EXPECTED` and `matches_the_full_six_by_nine_writer_matrix` exactly
as they are (its `assert_eq!(EXPECTED.len(), 54, ...)` line must not change). Add
`commit_is_relayed_only_by_a_stage_session`: its own independently transcribed six-row table
for the commit column, checked with `verdict`.

## 3. The payload (`relay/payload.rs`, `relay/mod.rs`)

```rust
/// `{message, paths}` for a `commit` request: the commit message and the
/// worktree-relative, `/`-separated paths to stage and commit. The daemon
/// validates both again (`relay::check_commit_request`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitRequest {
    pub message: String,
    pub paths: Vec<String>,
}
```

`RequestPayload::Commit(CommitRequest)`; `decode_payload` arm
`RequestKind::Commit => Ok(RequestPayload::Commit(decode(payload, "commit")?))`. Export
`CommitRequest` from `relay/mod.rs` beside `HandoffRequest`. Inline test
`decodes_a_commit_request_and_refuses_an_unknown_field`.

## 4. The shared validator (`relay/commit_paths.rs`, new)

Declared `mod commit_paths;` in `relay/mod.rs`, re-exported:
`pub use commit_paths::{check_commit_request, MAX_COMMIT_MESSAGE_BYTES, MAX_COMMIT_PATHS, MAX_COMMIT_PATH_BYTES};`

```rust
pub const MAX_COMMIT_PATHS: usize = 256;
pub const MAX_COMMIT_PATH_BYTES: usize = 1024;
pub const MAX_COMMIT_MESSAGE_BYTES: usize = 8 * 1024;

/// Every rule a commit request must meet before git runs: a message, and
/// paths that are normalized, relative, inside `worktree`, outside loom's
/// state and the session's tool configuration, and reached through no
/// symlinked directory. `worktree` is the canonical worktree root. The
/// error names the first path (or the message) that failed and why.
pub fn check_commit_request(request: &CommitRequest, worktree: &Path) -> Result<(), String>
```

Rules, in this order:

1. Message: not empty after `trim`, at most `MAX_COMMIT_MESSAGE_BYTES`, no NUL (an argv cannot
   carry one).
2. Paths: 1 to `MAX_COMMIT_PATHS` of them.
3. Each path, lexically: not empty; at most `MAX_COMMIT_PATH_BYTES`; no NUL; not starting with
   `/`; split on `/`, no empty component (so no `//`, no leading or trailing `/`), no `.` and no
   `..` component. Refusal: `path '<p>' is not a normalized path inside the worktree`.
4. Control roots: the first component is none of `.loom`, `.claude`, `.git`, `.worktrees`,
   `.work` (the legacy state link `git-add-guard.sh` also refuses), `.mcp.json`; and no
   component anywhere is `.git`. Refusal: `path '<p>' names loom state or tool configuration
   ('<component>')`. None of these is tracked in this repository (`git ls-files .loom .claude
   .mcp.json .worktrees .work` is empty at HEAD).
5. Symlinks: for each proper prefix of the path (`a`, `a/b` for `a/b/c`), `symlink_metadata` of
   `worktree.join(prefix)`: a symlink refuses with `path '<p>' goes through a symlink at
   '<prefix>'` (the phrase `through a symlink` is pinned by a contract); `NotFound` stops the walk
   (a deleted directory is a legitimate deletion, git reports anything else); another I/O error
   refuses naming it. The leaf itself may be a symlink: it is committed as a link (mode 120000),
   never followed.

Rule 5 is a by-name check, so it cannot be race-free: an agent can swap a component between the
check and any later open. It gives the CLI (W2) early feedback and the daemon a first filter. The
daemon's authority is the descriptor walk in section 5, which re-checks every component through
`openat(..., O_NOFOLLOW)` and never hands a path to git.

Inline tests (a `#[cfg(test)] mod tests` at the end of the file, TempDir paths only):
`a_normalized_relative_path_passes`, `dot_dotdot_empty_and_absolute_paths_are_refused`,
`control_roots_and_nested_git_dirs_are_refused`, `a_symlinked_parent_is_refused`,
`a_symlink_leaf_and_a_missing_parent_pass`, `an_empty_or_oversized_message_is_refused`,
`too_many_paths_are_refused`.

## 5. The daemon's commit (`inbox_drain/commit.rs`, `inbox_drain/commit_read.rs`, new)

Declare `mod commit;` and `mod commit_read;` in `inbox_drain.rs` beside `mod merge_resolved;`,
and the tests beside `#[cfg(test)] mod tests_merge;` as `#[cfg(test)] mod commit_tests;` and
`#[cfg(test)] mod commit_tests_files;`.

**The threat this design answers (security review, settled).** The daemon runs git on the host,
outside every sandbox, on paths the agent names. `git add` lstat()s a file and then open()s it
without `O_NOFOLLOW`, so an agent looping `rename(file, symlink to ~/.config/gh/hosts.yml)` can
make the daemon read a credential the sandbox hides and commit it; a FIFO or a huge sparse file
stalls the poll tick. `git add -- sub` on a nested repository makes a mode-160000 gitlink, and
later host-side `git status` recurses into `sub/.git` and runs the agent's filter. So the daemon
never runs `git add`, `git commit`, `git status` or any git command that reads the worktree. It
reads each file itself through a descriptor walk and hands git only blob contents and index
entries.

```rust
/// Where a relayed commit is made: the daemon's own state and repository, the
/// stage the entry is attributed to, and the requesting session's record.
pub(super) struct CommitSite<'a> {
    pub work_dir: &'a Path,
    pub repo_root: &'a Path,
    pub stage_id: &'a str,
    pub record: &'a Session,
}

/// Make the commit `request` asks for, or say why not. Never `Err`: every
/// failure is a refusal the ledger records.
pub(super) fn apply_commit(site: &CommitSite<'_>, request: &CommitRequest) -> Settle
```

Steps (each helper under 50 lines; the file under 400). Before step 1, at the start of every
apply, remove each leftover file in the daemon's staging directory `<work_dir>/commit-staging/`: a
daemon that crashed between writing a temp file and deleting it leaves one, and applies run one at
a time, so no file there is in use. The directory is created again (mode 0700) when absent.

1. **Owner and state.** Change `apply.rs::require_owner` to `pub(super)` and to return the loaded
   `Stage` (`Result<Stage, String>`); `stage_request` keeps ignoring the value. On top of it,
   refuse unless `stage.status == StageStatus::Executing` ("stage '<id>' is <status>; a commit is
   made only while it is Executing") and `stage.worktree` is `Some`.
2. **The worktree comes from the stage record, never the payload:**
   `crate::git::worktree::get_worktree_path(worktree_id, site.repo_root)`, then
   `WorktreeGit::pinned(site.repo_root, &path)`; an `Err` refuses ("the stage worktree is not a
   registered worktree: <error:#>"). `pinned` canonicalizes the worktree and checks it against
   `<common dir>/worktrees/*/gitdir` (`git/worktree/pinned.rs:66-79`).
3. `check_commit_request(request, repo.work_tree())` → refusal on `Err` (the lexical rules, and
   rule 5's by-name symlink check as a first filter).
4. **HEAD guard.** `symbolic-ref --quiet HEAD`: exit 0 with stdout (trimmed) equal to
   `format!("refs/heads/{}", branch_name_for_stage(stage_id))`
   (`crate::git::branch::branch_name_for_stage`, `git/branch/naming.rs:4`) passes; exit 1 is a
   detached HEAD; anything else refuses with "the stage worktree's HEAD is <x>, not
   refs/heads/loom/<id>; nothing was committed". Use the stage id for the branch, not the
   worktree id. Record `old = rev-parse --verify HEAD`.
5. **Reset the index to HEAD:** `read-tree --reset HEAD`. Under D1 a session cannot write its git
   administrative directory, so the index is the daemon's; this drops anything an earlier
   failed request left staged. `read-tree --reset HEAD` reads no worktree file content and keeps
   the stat data of every entry that matches HEAD. Never plain `read-tree HEAD`: measured on git
   2.53, it zeroes the cached stat data of EVERY index entry, and under D1 nothing can refresh
   `W/index` afterwards (the session cannot write it, and host readers run `--no-optional-locks`).
   Every later `git diff-index HEAD` in the worktree would then list every tracked file, and that
   output feeds `context/worktree_graph.rs::changed_paths`, the impact-test selection and the
   acceptance cache fingerprint.
6. **Read every path** (`commit_read.rs`, below) into a list of index entries (mode, blob id,
   path) and removals. Any refusal refuses the whole request; blobs already written stay as
   unreachable objects for gc. Limits: at most 256 paths (`MAX_COMMIT_PATHS`), 64 MiB per file,
   256 MiB per request.
7. **Update the index, argv only:** one `update-index --add --cacheinfo <mode>,<blob>,<path> ...`
   for every entry, then `update-index --force-remove -- <path> ...` for the removals. `--cacheinfo`
   takes the path literally (a comma in it is fine: the path is everything after the second
   comma) and reads nothing from the worktree.
8. **No gitlink:** `diff --cached --raw -z --ignore-submodules=none HEAD` (the flag overrides the
   `diff.ignoreSubmodules=all` below, which would otherwise hide a gitlink): any entry whose new
   mode is `160000` refuses. The staging above cannot create one; this is the backstop.
9. **Anything to commit?** `write-tree` gives the index's tree; equal to `rev-parse HEAD^{tree}`
   settles `Settle::Applied(Some("nothing to commit: every named path already matches HEAD".into()))`
   (a recorded result, not an error).
10. **Commit with plumbing:** `commit-tree <tree> -p <old> -m <message>` gives the new commit;
    `update-ref -m "commit (loom commit): <first line of the message>" refs/heads/loom/<id> <new> <old>`
    moves the branch only if it still points at `old` (a moved branch refuses). `git commit` is
    not used: it refreshes the index against the worktree, which reads files again.
11. **Result:** `Settle::Applied(Some(format!("committed {new}")))` with the full 40-hex id. That
    note is the ledger row's `reason`, which `loom request status` prints (W2).

On every refusal after step 5, run `read-tree --reset HEAD` again so the index is left at HEAD.

**Every git call in the apply** goes through one helper that prepends these to the arguments
(`run_git_with_env` already adds `-c core.hooksPath=/dev/null`, `git/runner.rs:71-77`):

```rust
/// Configuration every daemon git call in a commit carries: no signing, no
/// auto-maintenance rewriting the shared git directory, no fsmonitor, and no
/// submodule recursion into a nested repository the agent planted.
const GIT_HARDENING: [&str; 14] = [
    "-c", "commit.gpgsign=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false",
    "-c", "core.fsmonitor=false", "-c", "diff.ignoreSubmodules=all",
    "-c", "submodule.recurse=false", "-c", "status.submoduleSummary=false",
];
```

**`commit_read.rs`: reading a path without following anything.** Open the canonical worktree
root as a directory (`crate::fs::safe_fs::safe_open_dirfd(repo.work_tree())`, `fs/safe_fs.rs:59`;
check it opens with `O_DIRECTORY` and refuses a symlinked final component, else open it yourself
with `libc::open(..., O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)`). Then, per path:

- **Parents:** for each parent component, `openat(dirfd, name, O_RDONLY | O_DIRECTORY |
  O_NOFOLLOW | O_CLOEXEC)`. `ELOOP` or `ENOTDIR` refuses with `path '<p>' goes through a symlink
  at '<prefix>'` (race-free: the kernel refuses the link at open). `ENOENT` means the leaf is
  absent (a deleted directory). After opening each parent below the root,
  `fstatat(fd, ".git", AT_SYMLINK_NOFOLLOW)` succeeding refuses with `path '<p>' lies inside a
  nested repository at '<prefix>'` (the root itself holds the worktree's own `.git` file; skip
  it). Mirror the `unsafe` and `OwnedFd` idioms of `fs/safe_fs.rs::portable_open_walk`; it is
  private, and `open_safely` (`safe_fs.rs:242`) opens a whole relative path at once, which does
  not give you each parent to check for `.git`.
- **Leaf:** `fstatat(parent, leaf, AT_SYMLINK_NOFOLLOW)`:
  - regular file: `openat(parent, leaf, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)`, then
    `fstat` on the descriptor must again say regular (a FIFO swapped in opens at once under
    `O_NONBLOCK` and fails this; a symlink swapped in fails the open with `ELOOP`; a directory
    fails the type check). Size at most 64 MiB and within the request total, checked on the
    descriptor before reading. Mode `100755` when any execute bit is set, else `100644`.
  - symlink: `readlinkat(parent, leaf)` into a buffer of `PATH_MAX + 1` bytes; a target that fills
    the buffer is refused (`readlinkat` truncates silently, so a full buffer may hold a cut
    target); mode `120000`; the blob is the target string. Never open the target.
  - absent (`ENOENT` here or at a parent): a removal, but only if HEAD tracks the path: after
    step 5 the index is HEAD, so `ls-files -z -- :(literal)<p>` must print exactly `<p>`;
    otherwise refuse ("'<p>' does not exist and HEAD does not track it").
  - anything else (directory, FIFO, socket, device) refuses, naming the path and its type. A
    directory is refused: the CLI expands directories into files before the ticket is written.
- **Blob:** `WorktreeGit::run` takes no stdin (`git/worktree/pinned.rs:97-107`, owned by
  `host-git-integrity`), so the settled `hash-object --stdin` and `update-index --index-info`
  legs are argv-only here. Copy the bytes read from the descriptor (streamed, bounded) into a
  `tempfile::NamedTempFile` (a normal dependency, `loom/Cargo.toml:37`) created in
  `<work_dir>/commit-staging/` (mode 0700; the start-of-apply sweep above empties it). No session
  can write the state directory, so git hashes exactly the bytes read through the descriptor. Then
  `--attr-source=HEAD hash-object -w --path=<p> <temp file>` for a regular file (`--path` applies
  `.gitattributes` conversion as `git add` would) or `hash-object -w --no-filters <temp file>` for
  a symlink's target. Delete each temp file after hashing it.

  `--attr-source=HEAD` is a global git option (git 2.40 or later), so it goes after the helper's
  leading `-c` pairs and before the subcommand; the call's first argument stays `-c` and the
  runner's 15 s read deadline still applies. Measured: a pinned `hash-object --path` reads
  `<worktree>/.gitattributes`. A FIFO there blocks the call until the deadline kills it, which
  stalls every stage's poll tick and repeats per request, and `* filter=x` runs a clean filter
  defined only in the operator's global config. With `--attr-source=HEAD` the call returned at
  once and ran no filter. So: the daemon reads no worktree file except the named paths, through
  the descriptor walk; attributes come from HEAD's tree; a committed `.gitattributes` can still
  select a filter defined in `R/.git/config`, global or system config (an accepted limit).

Git failure text: stderr trimmed, capped at 2,000 bytes, prefixed with the git action. Use one
helper for it; do not copy `run_git_checked`'s multi-line format.

Traps:

- `git/runner.rs::git_timeout` picks the deadline from `args.first()`: every call here starts
  with `-c`, so all get the 15 s read deadline. That is enough to hash 64 MiB and write a tree;
  accept it and say so in a comment.
- Accepted limit: the daemon's own killed git can leave `index.lock` in the worktree's git
  administrative directory. Sessions cannot write that directory, so only the daemon's git can
  leave one; the next request's `read-tree --reset HEAD` then refuses with git's own `index.lock`
  message, and the refusal reason appends: "an operator must delete <admin dir>/index.lock (the
  daemon's git was killed mid-commit)", with `<admin dir>` the real absolute path of `W`
  (`rev-parse --absolute-git-dir`). The one git-failure-text helper adds the sentence whenever
  git's stderr contains `index.lock`. Do not add lock removal.
- After a commit, only the paths a commit staged lose stat data (their index entries come from
  `--cacheinfo`), so the next `git status` in the session re-hashes only those files (read-only
  there). That is expected; every other entry keeps its stat data because step 5 resets with
  `--reset`.
- `CommitSite` borrows: in `apply`, copy `host.repo_root().to_path_buf()` first, as the `Handoff`
  arm does (`apply.rs:69-78`).
- Keep `commit.rs` and `commit_read.rs` each under 400 lines and every function under 50. The
  literal `"add"` must not appear in either file; acceptance checks it. That check misses
  `update-index --add -- <path>` and `commit --only`, which make git lstat and then open the
  worktree path; the argv test `the_daemon_never_hands_git_a_worktree_file` (section 8) guards
  them.

## 6. Dispatch and the subagent rule (`inbox_drain/apply.rs`)

- `apply`: `RequestPayload::Commit(request) => commit::apply_commit(&CommitSite { .. }, &request)`.
- `admit`: before the matrix, refuse any control kind relayed by a subagent:
  `if inbox_entry.agent == AgentRole::Subagent && inbox_entry.kind.is_control()` →
  `Err(format!("a '{}' request from a subagent is never applied: only the session's main agent makes it", inbox_entry.kind))`.
  The relay hook already drops these (`commands/hook/relay.rs:332-344`); this is the daemon's own
  check, so a forged or future writer of `<work_dir>/inbox` cannot skip it. The phrase `subagent`
  is pinned by a contract.
- Update the module doc of `apply.rs` for the new handler.

## 7. Drain fixtures and the section-5 matrix (`test_support.rs`, `tests_matrix.rs`)

- `test_support.rs`: keep the existing signatures of `fixture` and `payload_for`; every change
  here is additive (a new match arm, a new method). capsule-policy's new
  `inbox_drain/tests_sweep_cache.rs` imports both while this stage runs in parallel.
  `payload_for(RequestKind::Commit)` returns
  `json!({"message": "test(relay): commit a.txt", "paths": ["a.txt"]})`. Add
  `Fixture::commit_worktree(&self, owner: &str) -> PathBuf`: calls
  `crate::verify::contracts::test_support::contract_worktree(&self.repo_root, STAGE)` (a repo on
  `main` with the worktree `.worktrees/s1` on `loom/s1`), sets `user.name`/`user.email` in the
  repository's own config (`contract_worktree`'s commits pass identity with `-c` only, and the
  daemon's pinned git reads the repository config), writes `a.txt` in the worktree, and saves
  `STAGE` Executing, owned by `owner`, with `worktree: Some(STAGE.to_string())`. Returns the
  worktree.
- `tests_matrix.rs`: `SECTION_5` becomes `[(SessionType, [Cell; 10]); 6]` with a tenth
  (commit) column: Stage `A`, every other row `R`; update the doc line listing the order.
  `run_cell` sets up `fx.commit_worktree(&record.id)` for `Commit`; `classify_applied` checks the
  stage branch gained one commit (`git rev-list --count main..loom/s1` is `1`, through
  `crate::verify::contracts::test_support::pinned(&worktree)`); `assert_no_effect` checks it
  gained none when `kind == RequestKind::Commit`. No existing `assert` line changes.

## 8. Unit tests (`inbox_drain/commit_tests.rs`, `commit_tests_files.rs`, new)

Drive the real pass (`super::run_pass(&mut host, &fx.tick(Utc::now()))`) with
`fx.relay(&record, RequestKind::Commit, payload)`, and read results from `fx.ledger(sid)` (take
the LAST row for the id with an outcome; see the mistake quoted below) and from pinned git. Every
refusal test also asserts the stage branch did not move and the index equals HEAD
(`diff --cached --quiet` exits 0).

`commit_tests.rs`:

- `a_stage_session_commits_the_named_paths_only` (another dirty file stays uncommitted; the note
  is `committed <sha of loom/s1>`; the commit message equals the request's)
- `nothing_staged_is_recorded_as_nothing_to_commit` (Applied, not Refused)
- `a_deletion_is_committed`
- `a_path_escaping_the_worktree_is_refused_before_git_runs`
- `a_control_root_path_is_refused` (`.loom/work/x`, `.claude/settings.json`, `.mcp.json`,
  `sub/.git/config`)
- `a_head_off_the_stage_branch_is_refused`
- `a_detached_head_is_refused`
- `a_stage_that_is_not_executing_is_refused`
- `a_session_that_does_not_own_the_stage_is_refused`
- `the_daemon_commit_ignores_repository_hooks_and_signing`: the plumbing the daemon runs
  ignores `commit.gpgsign` and never runs `pre-commit`, so a failing `pre-commit` or a signing
  setting alone cannot make this test fail. Plant a failing `reference-transaction` hook and a
  `post-index-change` hook that writes a marker file (both mode 0755), once in `R/.git/hooks` and
  once in a directory the repository's `core.hooksPath` names. Assert the request is Applied and
  no marker exists. Signing stays as an extra check: the repository config also sets
  `commit.gpgsign=true` and `gpg.program=false`, and the commit still lands.
- `a_stale_index_lock_is_reported` (an `index.lock` created in the worktree's git directory
  before the pass: Refused, the reason contains `index.lock` and `an operator must delete`)
- `a_commit_keeps_the_index_stat_data_of_untouched_paths` (the fixture tracks at least two
  files the request does not name and runs `git status` in the worktree first, so the index is
  refreshed; apply one modified tracked path; then `git diff-index --name-only HEAD` in the
  worktree prints exactly that path and no other: its entry comes from `--cacheinfo` and carries
  no stat data, which is the expected limit in the traps above. Plain `read-tree HEAD` makes it
  list every tracked file. Applying a deletion instead prints nothing)
- `the_daemon_never_hands_git_a_worktree_file` (`#[serial]`). Capture every git argv the apply
  runs: set `GIT_TRACE=<a file in the TempDir>` when the runner forwards it to git, otherwise
  prepend `PATH` with a logging `git` wrapper script that execs the real git; restore the
  environment afterwards. Commit a regular file, a symlink and a deletion. Assert that no logged
  subcommand is `add`, `commit`, `status` or `stash`, that every `update-index` carries
  `--cacheinfo` or `--force-remove`, and that every `hash-object` file operand lies under the
  daemon's staging directory. Reason: the acceptance check that `"add"` never appears misses
  `update-index --add -- <path>` and `commit --only`, which make git lstat and then open the
  worktree path.
- `a_control_request_from_a_subagent_is_never_applied` (an entry with `agent: Subagent`, built
  with `entry_for` then the field changed)
- `a_replayed_commit_entry_is_applied_once` (after the first pass, change `a.txt` again, plant
  the same entry's bytes with `fx.plant`; a second pass leaves the branch head and the ledger's
  outcome rows for that id unchanged)

`commit_tests_files.rs`:

- `a_symlinked_parent_is_refused_and_nothing_is_read_through_it` (the outside file's blob, from
  `git hash-object` without `-w`, is absent from the object store)
- `a_symlink_leaf_is_committed_as_a_link` (mode `120000`, blob equal to the target string, the
  target's content absent from the object store)
- `a_fifo_is_refused_without_blocking` (`libc::mkfifo`; the pass returns, Refused naming it)
- `a_directory_path_is_refused`
- `a_path_inside_a_nested_repository_is_refused` (`sub` holds a `.git` directory whose config
  defines `filter.x.clean` touching a marker, and `sub/.gitattributes` says `* filter=x`: Refused,
  the reason contains `nested repository`, no marker, no `160000` entry). Prove the planted
  filter fires before the drain: in the fixture run `git -C sub hash-object --path=a.txt a.txt`
  (with `sub/a.txt` present), require the marker, delete it, then run the drain. Without that
  control, "no marker" could mean the filter never worked. Trap: write the filter with
  `git -C sub config filter.x.clean '<cmd>'`, never by editing the config file by hand: git parses
  an unquoted `;` in a config value as a comment, so a hand-written `touch X; cat` silently breaks.
- `a_fifo_gitattributes_does_not_stall_the_commit` (a FIFO at `<worktree>/.gitattributes`, commit
  `a.txt`: the drain pass returns in under 5 s and the request is Applied. A watchdog thread
  opens the FIFO for writing after 5 s, which releases a blocked reader, and records that the
  timeout fired; the test fails when the timeout fired. Join the watchdog before the test ends)
- `an_oversized_file_is_refused_unread` (a sparse file set to 65 MiB with `File::set_len`)
- `an_executable_file_keeps_its_exec_bit` (mode `100755`)
- `a_missing_untracked_path_is_refused`
- `the_regular_leaf_open_refuses_a_symlink` (call the leaf-open helper directly on a symlink:
  `Err`, as when a regular file is swapped for a symlink between `fstatat` and `openat`)

The plan pins these tests by exact module path with `--exact`; keep each name as written:
`commit_tests::a_detached_head_is_refused`, `commit_tests::a_stale_index_lock_is_reported`,
`commit_tests::nothing_staged_is_recorded_as_nothing_to_commit`,
`commit_tests_files::an_oversized_file_is_refused_unread` and
`commit_tests_files::a_directory_path_is_refused`.

Git in tests: `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` at missing files and `GIT_CONFIG_NOSYSTEM=1`
for the fixture's own git commands, as `src/verify/impact_tests_tests.rs` does.

## 9. The relay hook's Rust side (`commands/hook/relay/tests*.rs`)

No production change: `Request::admit` (`commands/hook/relay.rs:332-353`) and the sweep
(`relay/sweep.rs`, control kinds excluded) follow `is_control()`. Add, mirroring
`a_control_kind_from_a_subagent_is_refused_even_when_allowed` (`tests.rs:195-204`):

- `tests.rs`: `a_commit_ticket_from_a_subagent_is_refused` (keep the file under 400 lines).
- `tests_sweep.rs`: `a_leftover_commit_ticket_is_never_swept`.

## Knowledge you need (quoted)

> Inbox Ledger Has Two Rows Per Request Id — a Lookup Must Take the Latest: the relay inbox
> drain appends an `applying` row when it starts processing a request, then an outcome row when
> it finishes, both under the same request id. A lookup that matches by id alone finds the
> `applying` row. Prevention: any reader of the inbox ledger must take the LATEST row for an id,
> never the first match. (`mistakes/concurrency-and-locking.md`)

> A control ticket stays bound to its own line: one a subagent wrote is refused, and sweeping it
> during a later main-agent call would relay the subagent's block, handoff or verdict under the
> main agent's authority. (`architecture/security-and-isolation.md`, "Session Requests Travel
> Through a Hook-Written Inbox")

## Maintainability

None of your files is in `loom/maintainability-baseline.txt` at `3fc28031`. Keep every file
under 400 lines and every function under 50. `apply.rs` is 324 lines today.

## Check

One run, after everything compiles: `cargo test --lib orchestrator::core::inbox_drain::`. W3 and
W4 edit other files in parallel; a compile error in a file you do not own is theirs: report it
with its file:line. Your code must satisfy the stage's contracts
`daemon-refuses-escaping-and-control-paths`, `daemon-refuses-a-symlinked-parent`,
`daemon-refuses-when-head-is-not-the-stage-branch`, `a-commit-request-applies-once`,
`a-subagent-commit-entry-is-refused`, `daemon-refuses-special-files-and-nested-repositories` and
`a-symlink-leaf-is-committed-as-a-link-never-its-target` (scenarios in the plan's YAML).
