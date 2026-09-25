# Ambient Filesystem Trust

> A .git dir is not evidence of a repo

## An Ancestor Named `.git` Is Not a Repository (2026-08-29)

**What happened:** `loom memory note` run from a directory with no repository in its own ancestry
wrote its journal into an unrelated directory far above it. On the machine where this surfaced,
that was the OS temp root, shared by every process and every concurrent agent on the box.

**Why:** `get_or_create_work_dir` accepted a candidate root on `root.join(".git").exists()`.
`find_repo_root_from_cwd` (`git/worktree/paths.rs`) walks up with NO ceiling, so from a repo-less
cwd it climbs to `/` and returns the first ancestor holding anything named `.git`. An empty
directory with that name — left by some unrelated tool — satisfied the check. The existence test
was guarding the wrong branch: it was written to catch the helper's "found nothing, returning cwd"
fallback, and did nothing about the "found something" branch it was actually reached through.

**Prevention:** an unbounded upward walk will eventually leave the territory you meant it to
search, so whatever it returns must be validated by STRUCTURE, not by name. Ask what the check
does when the named thing exists but is empty or foreign. Be especially suspicious of any walk
that can reach a shared root — `/tmp`, `$HOME`, a mount point — where unrelated processes leave
debris.

**Fix:** `is_real_git_dir` requires a `.git` DIRECTORY to also carry a `HEAD`, which every real
repository has from the moment it is created; a `.git` FILE is still accepted on existence alone,
since that is the worktree pointer form and git always writes real content there. Only the
creation path is affected; the reuse and read-only degrade paths were already incapable of
creating anything.

**How it was found, which is the more general lesson:** as 77 test failures across seven unrelated
modules. `WorkDir::new` also searches upward, so the one stray `.loom/work` this bug created at the temp
root was adopted by every test that built a `TempDir` beneath it. The failures looked environmental
and were not — they were a real production defect reported through an unrelated symptom. A test
failure whose cause looks like "the machine" deserves a root cause before it earns that label.

## `find_repo_root_from_cwd` Returns `Some(cwd)` Outside Any Repo (2026-08-11)

**What happened:** `get_or_create_work_dir` in `commands/memory/handlers/work_dir.rs` needed "am I inside a
git repo?" before it would create a `.loom/work` directory. `find_repo_root_from_cwd` returns
`Option<PathBuf>`, so `None` reads as "not in a repo" — but it is not. After walking to the
filesystem root without finding a `.git`, it ends at
`git/worktree/paths.rs:84-85` with an explicit _"Fallback: return the original cwd if nothing else
works"_ → `cwd.canonicalize().ok()`. Outside any repo it therefore returns `Some(cwd)`.

**Why it matters:** the name says _find repo root_ and the `Option` implies a search that can
fail, so `if let Some(root)` looks like a repo check and compiles clean. Here it would have
scattered a `.loom/work` directory into any directory the command was ever run from.

**Prevention:** treat `find_repo_root_from_cwd` as _"the best base path to use"_, never as a repo
predicate. When you need the predicate, confirm it yourself:
`find_repo_root_from_cwd(&cwd).filter(|root| root.join(".git").exists())`.

**Detection:** the giveaway is an `Option` whose `None` arm you cannot trigger in a test. If you
cannot write the failing case, the function probably never returns `None` — read its tail before
relying on it. Existing callers are unaffected only because they all pair it with
`.unwrap_or_else(|| cwd)`, which wants exactly the fallback; that idiom hides the trap from anyone
reading call sites to infer semantics.

## Hook Install Fabricated `.git/`, Then `git init` Refused the Leftovers (2026-09-11)

**What happened:** in a directory that was not yet a repository, `loom repair --fix` followed by
`loom init <plan>` failed with `git init failed ... could not lock config file .git/config: File
exists`. The project's `.git/` held `loom-hooks/pre-commit`, three 0-byte files (`config`,
`config.lock`, `config.worktree`) and no `objects/`.

**Why (three links):**

1. `install_pre_commit_hook` (`git/hooks.rs`) ran `create_dir_all(".git/hooks")` without checking
   that a repository existed, and repair (`commands/repair/hooks.rs`) raised "pre-commit hook not
   installed" in any directory. `loom repair --fix` therefore fabricated a `.git/` skeleton.
2. Claude Code's Linux sandbox write-protects `<cwd>/.git/config`, `config.lock`,
   `config.worktree`, `config.worktree.lock`, `hooks` and more (list read from the 2.1.269
   binary). Once `.git/` existed, sandboxed commands in that project left 0-byte placeholder
   files at those paths behind.
3. `ensure_repo_ready_for_worktrees` (`git/repository.rs`) treats a failed `rev-parse` as "no
   repo" and runs `git init`, which refuses to start while `config.lock` or `HEAD.lock` exists or
   while `config` does not parse.

**Misleading signals:** the user blamed `loom repair --fix` because it ran just before; the 0-byte
files date from separate sandboxed commands minutes later. A failed `git init` still writes `HEAD`,
`refs/`, `info/` and the sample hooks before it dies on the lock, so the skeleton afterwards passes
the `HEAD` test of `is_real_git_dir` (first entry above). Only `objects/` is missing.

**Prevention:** code that writes under `.git/` must first confirm `.git` is a directory it did not
have to create. Code that runs `git init` over an existing `.git/` must expect debris: a lock with
no live owner, an empty or unparseable config. Probing `git init` in a scratch directory holding
the exact debris settles which leftovers matter: on git 2.43 only `config.lock`, `HEAD.lock` and an
unparseable `config` block it; an empty `config`, a junk `config.worktree` and `index.lock` do not.

**Fix:** when `rev-parse` fails and `.git/` is a directory, `git/init_blockers.rs` deletes
`config.lock`/`HEAD.lock` and renames a `config` that `git config --file` cannot parse to
`.git/config.loom-backup-<secs>` before `git init` runs. The same path recovers a real repository
whose config was corrupted, history intact. `install_pre_commit_hook` bails when `.git` is not a
directory and repair raises the hook issue only inside one; `loom init` bootstraps git before its
startup repair installs the hook.

## An Empty `.git` Left at a Shared TMPDIR Root Re-Bounds Every Test Beneath It (2026-09-12)

An empty `.git` directory (no `HEAD`, `config`, or `objects`; not created by any stage script)
appeared at the sandbox's `TMPDIR` root itself (e.g. `/tmp/claude-1000`). `fs::work_dir`'s
`nearest_git_root` treats any `.git` entry as a repo boundary regardless of contents, so
`fs::work_dir::tests::resolver::a_workspace_above_a_git_free_directory_is_never_adopted` failed
deterministically for any `TMPDIR` nested under it — 30/30 runs after the directory appeared,
against 2/2 full runs and 19/20 isolated runs clean before. `fd`/directory listings inside the
temp root never surface the root's own `.git`; the fix was to `stat` every ancestor of `TMPDIR`
for a `.git` entry, not just search inside it.

**Prevention:** run `cargo test` with `TMPDIR` at a path whose ancestors hold no `.git` (a plan's
dedicated sandbox grant, e.g. `/tmp/loom-pre-commit-plan`, works if nothing has left one there).
Before treating a resolver test as flaky, check every ancestor of the active `TMPDIR` for a
stray `.git` first.

**Open question:** should `nearest_git_root` require a real repository marker (a `.git` dir
containing `HEAD`, or a `.git` file starting with `gitdir:`) instead of bare existence? That
would stop a bare `.git` from silently re-bounding the walk for everything beneath it, but the
existing `bare_repo` test helper plants exactly such an empty directory and would need updating
alongside the change.

## Two More Walkers Trusted an Empty `.git`, and the Sandbox Temp Root Held One (2026-09-13)

**What happened:** `fs/work_dir.rs::nearest_git_root` bounded the upward workspace search at any
ancestor holding a `.git` entry, and `skills/project/scan.rs::checkout_root` used the same
existence test to choose the project-scan root. During one session `/tmp/claude-1000/.git` existed
as an empty directory under Claude Code's per-user sandbox temp root (`$TMPDIR` when sandboxed) and
later vanished. While it existed, every `TempDir` there saw `/tmp/claude-1000` as its repository, and
`skills::project::tests::infrastructure_markers_remain_detectable` picked up a `typescript` marker
from unrelated files elsewhere in the temp root.

**Fix:** `is_real_git_dir` moved to `fs/git_marker.rs`; `nearest_git_root`, `checkout_root` and the
scan's nested-checkout skip all use it. Fixtures that plant `.git` as an empty directory now also
write `HEAD`.

**Second trap in the same walk:** when no real `.git` is found, `checkout_root` falls back to the
nearest ancestor that is a package boundary. Another session left `/tmp/claude-1000/package.json`,
so a test repo with neither a `.git` nor a manifest of its own still climbed to the shared temp
root. The fallback is right for real non-git projects; the test was not hermetic. A test that calls
`ProjectProfile::discover` gives its `TempDir` a real `.git/HEAD` or a manifest.

**Prevention:** every upward walk that stops at a `.git` goes through
`fs::git_marker::is_real_git_dir`. A test that can reach its `TMPDIR`'s ancestors is exposed to
whatever other processes leave there, so bound the walk inside the test's own directory.

**Further bound (2026-09-13):** `WorkDir::new` (`fs/work_dir.rs`; the walk itself is `walk_up` in `fs/work_dir/discovery.rs`) now also never inspects the
OS temp root (`std::env::temp_dir()`, canonicalized) or anything above it when the base path is inside
it — on top of the `is_real_git_dir` check above. See [Live State Pollution](live-state-pollution.md),
which this bound was added to fix.
