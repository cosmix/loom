# Parallel Worktree Shared State

> Cross-worktree state races: the one diagnostic question, concrete cases, and a blind-review-subagent instance.

## The One Question That Catches This Whole Class

**"Was the path I am about to write resolved through `main_project_root` (or through
the `.work` symlink)?"** If yes, it is NOT inside your worktree — it is shared with
every sibling stage and with the main repo, and writing it escapes worktree isolation.

Derived context state is the main offender because its whole point is to be shared.
Four separate defects in one plan trace to this single fact.

## The Concrete Cases

- **`ContextStore::open` follows the `.work` symlink** (`context/store.rs:49`), so ANY
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
  `.work/context/<plan>/<stage>/`, so the dependency-ranking boost failed 100% of the
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
them came back blind: `hooks/worktree-file-guard.sh` blocks every file tool on any path outside
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
