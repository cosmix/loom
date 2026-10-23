# Parallel Worktree Shared State

> Cross-worktree races

## The One Question That Catches This Whole Class

**"Was the path I am about to write resolved through `main_project_root` (or through
the `.loom/work` symlink)?"** If yes, it is NOT inside your worktree — it is shared with
every sibling stage and with the main repo, and writing it escapes worktree isolation.

Derived context state is the main offender because its whole point is to be shared.
Four separate defects in one plan trace to this single fact.

## The Concrete Cases

- **`ContextStore::open` follows the `.loom/work` symlink** (`context/store.rs:49`), so ANY
  command that opens the store writes under the MAIN project root, not the worktree.
  From a sandboxed worktree session that surfaces as
  `Failed to create context cache directory: <main repo>/.loom/cache/context-v1:
  Read-only file system (os error 30)`. **The path is gitignored derived state and the
  command is not broken** — do not "fix" the command.
- **Parallel worktrees therefore share ONE cache.** An interleaved save could leave one
  worktree's `catalog.json` paired with another worktree's `state.json`, serving foreign
  chunks labelled `current`.
- **A runtime "ensure X is ignored" helper escapes the worktree.** Reusing
  `add_to_gitignore_exclude` would append to `.git/info/exclude` under
  `main_project_root` on every acceptance run. A committed `.gitignore` line lives
  inside the worktree, merges back normally, preserves foreign rules, and needs zero
  runtime mutation. **Before adding any runtime ignore helper, check whether its target
  resolved via `main_project_root`.**
- **A discard routine deleted a shared directory rather than its own layer.**
  `discard_overlay` removed delivery records that live alongside the graph layer under
  `.loom/work/context/<plan>/<stage>/`, so the dependency-ranking boost failed 100% of the
  time on the daemon path. **A "discard the derived layer" operation must name the
  layer, never the directory** — enumerate what else writes there first.

Related trap in the same area: `GraphStore::overlay_dir`
(`context/graph_store/mod.rs:180`) derives its path from the work root ALONE — the
`context_cache_root` passed to `GraphStore::new` never reaches it. Calling
`base_dir`/`base_path` on a store built that way yields nonsense, so keep such a store
scoped to a private helper.

## Two Files That Must Agree Need the Agreement Recorded, Not Just Locked

`store.rs` saves `catalog.json` and `state.json` separately under independent locks, and
`evaluate()` read `state.json` alone without checking the catalog it described. Deleting
`catalog.json` while keeping `state.json` made `status` print "never built" and "current"
in the same output, and made every query silently re-ingest in memory while reporting the
cache fresh.

**Rule:** when two files must agree, record the identity of one INSIDE the other (or in
shared state) and have the reader verify the pair before trusting either. A per-file lock
protects each write and never the invariant BETWEEN them. The follow-on fix held one lock
across the whole `state.json` read-modify-write.

**Corollary on locking:** read AND write inside the same `locked_dir_update` closure
using plain `fs::read_to_string` — the locked read helpers take the same directory lock
and would deadlock. And `record_delivery` must MERGE within one `context_epoch`, not
replace: the prompt hook keys on a stable recipient id (`prompt-<stage>`), so a replacing
write erases the previous delivery set and the next prompt re-quotes it verbatim.

## When Two Values Describe "The Same Thing", Say Which One Persists

A stage spec pinned TWO revision hashes without stating which is persisted, and the
incremental path silently broke: `catalog.revision` hashes `<chunk id>:<content_hash>`
lines, `tree_revision` hashes `<relative path>:<file content_hash>` lines. `refresh()`
stored `catalog.revision` while `evaluate()` compared `tree_revision`, so they never
matched and **every refresh rebuilt from scratch**.

The misleading signal: both are hex sha256 strings of identical shape and both
round-trip fine, so unit tests of each module in isolation pass. Only a cross-module
idempotence test caught it.

**Rule:** when a spec defines more than one hash or revision over the same subject,
state explicitly which value is PERSISTED and which it is COMPARED against, and require
one test that runs the operation TWICE and asserts the second is a no-op.

## Codex Leaves Empty Dotfiles At The Worktree Root

A codex lane run inside a loom worktree leaves eleven HOME-shim dotfiles at the WORKTREE ROOT,
created in one sub-10ms burst: `.bashrc` `.zshrc` `.profile` `.bash_profile` `.zprofile`
`.gitconfig` `.gitmodules` `.mcp.json` `.ripgreprc` `.idea` `.vscode`.

They look like the user's real dotfiles leaking into the repo, or a rogue agent writing outside
its file set. They are neither — codex sandboxes HOME onto the workspace root so a sandboxed
shell reads inert configs instead of the real ones.

**Precise detection signature** (observed 2026-08-17, `ls -la`):

```text
crw-rw-rw- nobody nogroup 0 B  .bashrc
```

They are **character devices** — `/dev/null` bind-mounted over each path — owned by
`nobody:nogroup`, not empty regular files. Either form shows up in `git status --short` as
untracked entries at the repo root. The `c` in the mode column is the fastest way to tell them
from real dotfiles someone actually created.

**Do NOT `git add -A`.** That is exactly how an empty `.gitmodules` gets committed to the repo
root and breaks submodule handling for everyone. Stage only your named files.

They survive the codex run, so a LATER stage in the same worktree inherits them and sees them in
its own `git status` with no codex run of its own to explain them — do not go looking for the
agent that wrote them.

## A Read-Only Review Subagent Staged Outside the Worktree Is Silently Blind

**What happened:** six read-only `loom-code-reviewer` subagents were spawned with their diffs
prepared under `$TMPDIR` (e.g. `/tmp/claude-*/diffs`) instead of inside the worktree. Every one of
them came back blind: `loom-hooks/worktree-file-guard.sh` blocks every file tool on any path outside
the current worktree, and `loom-code-reviewer`'s agent type carries no Bash tool to work around
it — so each reviewer's Read calls all failed, and their reports read as generic
pattern-matching rather than an actual review of the diff.

**Why it is dangerous rather than merely wasteful:** a blind reviewer does not fail loudly. It
still produces prose that reads like a review, so the orchestrator can mistake "the reviewer
returned findings" for "the reviewer actually looked," and complete the stage on unverified work.

**Prevention:** a review subagent's input must live INSIDE the worktree it is spawned from.
Stage the prepared diffs at `.worktrees/<stage>/.review-diffs/` (or equivalent), pass the
reviewer the RELATIVE path, and delete that scratch directory before committing — never a
`$TMPDIR` or other outside-worktree path, even for content the orchestrator itself generated.
When a review subagent's report looks suspiciously generic or reads as boilerplate, check first
whether it could actually reach its input before trusting the content of the finding.

## A Pathless `git commit` Took Another Agent's Staged Files (2026-09-13)

**What happened:** four commits were made from the main checkout while other agents worked in it.
The index was checked for staged changes once, before the first commit. During that commit's
pre-commit hook (about a minute of formatting, ledger and rustdoc checks) another agent staged
`web/dist/favicon.svg` and `web/public/favicon.svg`, and the next `git commit` recorded them under
`fix(skills): stop trusting an empty .git as the project scan root` (`a9b412b3`).

**Why:** `git commit` with no paths records the whole index, and every agent working in a checkout
shares that index. A clean-index check goes stale as soon as anything else runs.

**Prevention:** in a checkout other agents also use, name the paths on the commit itself:
`git add -- <paths> && git commit -- <paths>`. Given paths, git commits only those files and leaves
anything else staged where it was. Afterwards compare `git show --stat HEAD` with the list you
meant to commit.

## `git stash push -- <file>` Discarded the Stage's Own Edits (2026-09-19)

**What happened:** to check whether a test failure was pre-existing, `git stash push -u -m <tag> -- <single-file>`
was run on a tracked file that already carried the stage's uncommitted edits. The file reverted to HEAD, taking
the stage's in-progress work with it (recovered with `git stash apply <sha>` then `git stash drop <sha>`). The
same slip was made twice in one stage.
**Prevention:** never stash one file to ask "is this failure pre-existing". Read the old bytes with
`git show HEAD:<path>` or a scratch copy. The stash stack is shared with every worktree, so stash only when the
whole tree must be set aside, tag the entry, and restore it by SHA with `git stash apply`.

## Do not reset a repository shared with active agents

**What happened:** An isolated feature commit was attached to main with git reset --mixed. The user then explicitly prohibited repository resets because other agents were working concurrently.

**Why:** The isolated checkout protected unrelated files from repository-wide commit hooks, but moving the primary branch and index with reset still affected shared Git state.

**Prevention:** Never run git reset in this shared repository. Preserve other agents staging and branch state; coordinate before integrating an isolated commit when ordinary safe integration is unavailable.

**Fix:** No further reset operations. The completed mixed reset preserved working files; the index had been checked empty before it ran. Feature commit: d2fed815.

## Claude Code's Bash Sandbox Shows The Same Placeholders, In Its Own Namespace

**What happened:** every `loom stage contracts freeze` a Claude Code contract writer attempted
was refused: the freeze's changed-paths check saw eleven untracked dotfiles at the worktree root
(`.bashrc .bash_profile .gitconfig .gitmodules .idea .mcp.json .profile .ripgreprc .vscode
.zprofile .zshrc`) the writer never touched, so a v2 stage with contracts could never leave the
contract phase.

**Why:** Claude Code's `bwrap` Bash sandbox bind-mounts `/dev/null` over the same fixed set of
worktree-root dotfiles the Codex sandbox does (above), visible only inside its own mount
namespace. On the host each mount point is an empty regular file while a sandboxed command runs,
and is sometimes left behind after it (`verify/tool_artifacts.rs` module doc); left-behind stubs
blocked post-merge worktree cleanup until `git/cleanup/worktree.rs::remove_sandbox_stubs` removed
the empty, untracked ones. `git status` reads the DIRECTORY ENTRY at each
path, which inside the sandbox's namespace is an ordinary mount point, so git reports it `??`
(untracked) exactly as it would a file the writer actually created; `lstat`/`symlink_metadata` on
that same path sees the character device the bind-mount put there — the dirent and the inode tell
different stories, and only the inode check can tell them apart. Git itself never lists a real
FIFO, socket, or device node a session might plant — none of those show up in `git status
--porcelain` at all — so a FIFO fixture alone cannot reproduce this bug; only a genuine device
node (the bind-mount itself, or `mknod`, which needs `CAP_MKNOD` no session has) does.

**Prevention:** an in-session git-status reader that must treat "untracked" as "the writer created
this" has to drop untracked artifacts at the eleven protected root names specifically, never every
unexpected untracked path — `verify::tool_artifacts::is_tool_artifact`
(`loom/src/verify/tool_artifacts.rs`, replaces the removed `git::branch::is_device_node`) matches
a listed name only when `symlink_metadata` finds it a char/block device, an empty regular file, an
empty directory, or gone (the mount point seen from inside the sandbox, from the host, or after
cleanup), applied to `changes::changed_paths`/`status_paths` (`verify/contracts/changes.rs`),
`list_working_tree_changes` (`git/branch/status.rs`) and `fingerprint::compute_local`
(`verify/review/fingerprint.rs`). The daemon's host-side `changes::special_files` walk
(`verify/contracts/special_walk.rs`) is the counterpart: it refuses any FIFO, socket or device
node found anywhere in the worktree, except a device-node placeholder at one of the same eleven
root names (`sandbox_mount`, the same `is_tool_artifact` rule) — a FIFO or socket an agent plants
at one of those names is still refused, since making a real device node needs CAP_MKNOD no session
has. Regression tests feed the parser a `?? .bashrc`-style line with the host's `/dev/null`, a real
character device, standing in for the mount, plus an empty file and an empty directory at other
listed names; a mkfifo fixture at a listed name is kept, since git skips FIFOs on its own but a
session-planted one must still count as a change.

**Fix:** `is_tool_artifact` gate added to every in-session git-status reader that lists untracked
paths for a contract freeze or a review fingerprint, and to the daemon's special-file walk.

## Landing verified work after task steering

**What happened:** The operator requested landing the reviewed macOS test-host branch on main and resuming the original full-suite verification task.
**Why:** Diagnosis required a separate branch; its final verification also completed the original five Rust gates.
**Prevention:** Check ancestry and retained evidence, preserve unrelated working-tree files, and require fresh verification if integration changes implementation sources.
**Fix:** Choose an ancestry-checked fast-forward of the reviewed commits, retain their normal-hook history, and compare the landed source tree with the verified branch before declaring completion.

## Full-suite evidence must follow the last runner edit

**What happened:** The completed Rust run preceded a final optional-environment-flag change in its Python launcher; an unchanged Rust tree alone did not satisfy the requested final-source verification.
**Why:** The evidence comparison covered Rust implementation identity but did not account for the executable verification harness timeline.
**Prevention:** Include runners and environment setup in the final-source boundary. A focused wrapper check supplements, rather than replaces, a requested fresh complete suite after implementation changes.
**Fix:** Run the bounded full Rust gates on main after all testing fixes are committed, retain fresh logs and exits, and preserve the earlier successful run as historical evidence.

## Strict verification controls need compatible helper tests

**What happened:** Enabling LOOM_TEST_REQUIRE_SANDBOX_FREE on macOS failed the helper unit fixture and six pre-existing Linux-only confinement guards across library and integration targets.
**Why:** The helper fixture assumed the default skip policy; the global strict flag also treats an unsupported operating system as a failure, so it cannot certify Linux cases on macOS.
**Prevention:** Check the strict flag scope before enabling it. Preserve unsupported-platform outcomes explicitly and retain real native confinement controls.
**Fix:** Preserve the failed strict invocation, correct the helper fixture to assert its selected policy without changing production behavior, and run the requested native gates without the optional global flag. Report the six Linux-only cases separately from executed macOS confinement tests.

## Format a fixture repair before the complete gate

**What happened:** The fresh suite passed after the strict-helper fixture repair, but cargo fmt --check then rejected the layout of its long expected panic string.
**Why:** The small test edit went straight to execution before its required formatting check.
**Prevention:** Format repairs before the complete gate so later checks do not invalidate source fingerprints or cause avoidable host load.
**Fix:** Apply only the formatter-required line wrapping, retain the failed formatting log, and verify formatting plus the remaining gates against the corrected source.

## Generated-asset patches need exact line endings

**What happened:** A task-only staging patch generated with Python did not apply to the minified web bundle, and the next file header was absorbed into a line lacking its final newline.

**Why:** Python `splitlines` recognizes Unicode separators that Git does not, and unified diffs need an explicit no-newline marker for unterminated lines.

**Prevention:** Split patch input only at LF, preserve unterminated lines with the standard marker, and run `git apply --cached --check` before staging a generated patch.

**Fix:** Repair the patch generator and validate both scoped patches against an isolated index before changing the shared index.

## Shared Cargo output can replace a verified executable

**What happened:** A sandbox check passed with the fixed executable, then appeared to regress when the same debug path was invoked later. Both the replaced executable and a copy captured from a subsequent build named another checkout and the old implementation in their backtraces.

**Why:** Concurrent checkouts shared the Cargo target directory. A later Cargo invocation treated the shared artifact as fresh, so copying its compiler-artifact event still captured the wrong build.

**Prevention:** Use a separate Cargo target directory for final command verification when concurrent checkouts share output. A pathname or a successful fresh-build report alone does not establish source provenance.

**Fix:** Build with explicit manifest and task-specific target paths, then verify that isolated executable.

## Committing From a Checkout Other Sessions Share Swept In Their Files

**What happened:** asked to commit the custom-root removal from the main checkout while another session, and a third unknown one, had uncommitted work there, the main agent built its file list as "every modified path minus the peer's announced files". Six `web/` files from the unknown session went into the commit. The knowledge commit also took `INDEX.md` rows for the peer's uncommitted topics, because `INDEX.md` had been regenerated from the whole working tree before the commit. Both commits were reset or amended before anything was pushed.
**Why:** a denylist excludes only the work you know about. `INDEX.md` is regenerated by every knowledge write or sync in any session, so its working copy carries everyone's rows.
**Prevention:** build the commit list as an allowlist of the files your own change touched (your diff plus the implementers' reports), and compare `git diff --cached --name-only` against it before committing. For `INDEX.md`, stage HEAD's version with only your topics' rows changed, built right before the commit. The pre-commit hook (`loom/.githooks/pre-commit`) refuses a path with both staged and unstaged changes, so a shared file is committed by writing your-hunks-only content, committing, then restoring the other hunks.
**Fix:** `git reset --soft HEAD~1`, unstage the foreign paths, recommit; amend the knowledge commit with a rebuilt `INDEX.md`.

## Runtime and Daemon Incident History

> Merge cleanup, daemon singleton, broker, waiting-state incidents

## Cleanup Inside "Merge" Destroyed the Evidence

`attempt_auto_merge` deleted the worktree and branch inside its own success arms — the branch its caller needed to verify the merge. Ask of any function with irreversible side effects: **after this returns, what can no longer be verified?** Also: cleanup refusing over scaffold it never planted, and over loom's own memory spool. → [Merge Cleanup Boundary](merge-cleanup-boundary.md)

## A Tracked Symlink Named `<file>.tmp` Redirected a Locked Write (2026-09-22)

`atomic_write`/`atomic_write_locked` opened `<path>.tmp` without `O_NOFOLLOW`, so a TRACKED symlink
named `<target>.tmp` redirected a locked write outside the repo. Directory-component following is
still a residual gap (`concerns/sandbox-and-confinement-gaps.md`).

→ [Sandbox & Settings](sandbox-and-settings.md)

## Completion Broker: a Server-Side Fallback the Client Could Never Reach (2026-08-11)

No worktree stage could complete through the trusted PostToolUse broker: the client sent an EMPTY credential when `user.token` was unreadable, trusting a daemon peer-identity fallback — but the wire preface refuses to frame an empty credential, so the request never left the process. Rule: a designed fallback must be traced end-to-end through every framing layer, and "absent" needs an explicit non-empty wire encoding.

→ [Completion Broker Credential](completion-broker-credential.md)

## Two Daemons Once Attached to the Same `.loom/work/` (2026-08-08)

Nothing enforced daemon singleton, so a second daemon could attach to a live `.loom/work/` and both would drive the same stages. Startup now takes an authoritative `flock` for the daemon's whole lifetime before touching the socket or control files. The daemon starts by re-executing `loom run --daemon-child`: forking it after threads existed crashed it on macOS while `loom run` reported success.

→ [Daemon Singleton Incident](daemon-singleton.md), [Fork After Threads](daemon-fork-after-threads.md)

## Adjudication Autonomy Deadlock (2026-09-02)

Session adoption matched by `stage_id` alone, an unconditional re-queue, a disputing agent never retired, no watchdog for "Executing with nobody working" combined into a 90-minute deadlock.

→ [Adjudication Autonomy Deadlock](adjudication-autonomy-deadlock.md)

## Status Broadcast Hardening: Frame-Overflow Eviction and Read-Timeout Desync

Widening the daemon's status push exposed two latent defects: an oversized payload silently evicted every socket subscriber, and a 50ms read timeout could desync the stream permanently.

→ [Status Broadcast Hardening](status-broadcast-hardening.md)

## A Stage's Test Run Rewrote Live State (2026-09-13) [DETAILED]

Tests run by a stage adopted the live `.loom/work` through `WorkDir::new`'s upward walk (TMPDIR nested in the checkout) and wrote a sticky marker that overrode `[terminal] backend`. Stage sessions must not write `.loom/`; no marker may override a configurable setting.

→ [Live State Pollution](live-state-pollution.md)

## Spurious waiting-for-input stages (2026-09-14) [DETAILED]

Stages flipped to `WaitingForInput` with no AskUserQuestion in any transcript, because the ask-user hooks acted on any invocation of the AskUserQuestion permission pipeline without reading stdin, and nothing reconciled the state; `loom stage complete` was then refused. The monitor now resumes a waiting stage whose own session keeps executing tools, and the hooks check `tool_name` and log every trigger. A second cause (2026-09-16): the SubagentStop lifecycle heartbeat carried no `subagent` flag, so the reconciler resumed a real wait; lifecycle heartbeats now set the flag and the reconciler requires a named tool.

→ [Spurious waiting-for-input stages](spurious-waiting-for-input.md)

## An Unbounded Walk Up the Filesystem Adopts Whatever It Finds (2026-08-29)

`loom memory note`, run outside a repository, wrote its journal into an unrelated ancestor directory because the repo-root search walks up without a ceiling and validated the result by NAME rather than structure. Also: `find_repo_root_from_cwd` returning `Some(cwd)` outside any repo, and the pre-commit hook installer fabricating `.git/` in a non-repo, after which `git init` refused the sandbox's 0-byte `config.lock` placeholder (2026-09-11).

→ [Ambient Filesystem Trust](ambient-filesystem-trust.md)

## Parallel Worktrees Share Derived State

One question catches the class: **was this path resolved through `main_project_root` or the
`.loom/work` symlink?** If so it is shared with every sibling stage and the main repo.

→ [Parallel Worktree Shared State](parallel-worktree-shared-state.md)

## A Fix Was Rebuilt on a Branch Whose Upstream Already Carried It

**What happened:** integration-verify started to reimplement the contract cached-stdout-tail fix that the operator had already committed on token-optimization (5753d07a). This worktree branched before that commit. The worker was told to drop its work, and its edits were restored from git.

**Why:** the stage read the memory of an earlier stage and the tree, not the newer commits on the target branch.

**Prevention:** before fixing a defect an earlier stage recorded, run `git log <stage-base>..<target-branch>` for an upstream fix.
