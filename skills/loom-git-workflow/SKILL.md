---
name: loom-git-workflow
description: Git operations guidance including branching strategies, commit conventions, merge workflows, conflict resolution, and worktree management. Use for any Git-related task from branch design to history rewriting.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
triggers:
  - git
  - branch
  - commit
  - merge
  - rebase
  - pull request
  - PR
  - cherry-pick
  - stash
  - reset
  - revert
  - checkout
  - switch
  - worktree
  - conflict
  - GitFlow
  - trunk-based
  - feature branch
  - release branch
  - version control
  - squash
  - amend
  - history
  - tag
  - remote
  - push
  - pull
  - fetch
  - clone
  - rerere
  - reflog
  - bisect
---

# Git Workflow

## Overview

Git operations with an eye to a clean, bisectable history and safe collaboration. Canonical home for **commit hygiene and conventional commits** (referenced by `/loom-code-review` and `/loom-refactoring`).

## The golden rule

**Never rewrite history that others have based work on.** `rebase`, `commit --amend`, `reset --hard`, and `push --force` on a shared/public branch orphan collaborators' commits and force painful recovery. Rewrite freely on your *own* un-pushed / unshared branch; treat anything pushed to a shared branch as immutable.

- Force-push only your own feature branch, and prefer `--force-with-lease` (aborts if someone else pushed since your last fetch) over `--force`.
- Never force-push `main`/`master`/`release/*` — protect them server-side.

## Commit hygiene (conventional commits)

Format: `type(scope): description` — `feat`, `fix`, `docs`, `refactor`, `test`, `chore` (+ `perf`, `build`, `ci`, `style`). `!` or a `BREAKING CHANGE:` footer marks an incompatible change.

- **Atomic:** one logical change per commit. It should build and pass tests on its own — a green history is a bisectable history.
- **Separate refactor from behavior change** (see `/loom-refactoring`): a `refactor:` commit is a runtime no-op; a `fix:` commit changes behavior. Mixing them hides the fix from reviewers and poisons `git bisect`.
- **Message = what + why, not how.** The diff shows how. Body explains motivation, tradeoffs, and links issues.
- **Group by concern:** 5 files across 3 concerns → 3 commits (module / its tests / wiring), not 1 monolith or 5 fragments.

```bash
git commit -m "fix(cart): resolve race in quantity update

Rapid add/remove let the count go negative via unsynchronized state.
Guard with an atomic compare-and-swap.

Fixes #234"
```

⚠ Loom stages: never `git add -A` / `git add .` — that stages the `.work/` symlink. Stage specific files only.

## Rebase vs. merge — pick deliberately

| Situation                                          | Use                              | Why                                                        |
| -------------------------------------------------- | -------------------------------- | ---------------------------------------------------------- |
| Update your local feature branch with `main`       | `rebase`                         | Linear history; your commits replay on top, no merge noise |
| Integrate a reviewed feature into `main`           | `merge` (often `--no-ff`)        | Preserves the branch as a reviewable unit; never rewrites shared history |
| Branch already pushed & others pulled it           | `merge` (NOT rebase)             | Golden rule — rebasing rewrites shared commits             |
| Clean up messy local WIP commits before a PR       | `rebase -i` (before pushing)     | Squash/reorder/reword while still private                  |
| Long-lived branch, want to record integration points | `merge`                        | Merge commits document when integration happened           |

Rule of thumb: **rebase to keep *your private* branch current; merge to *share*.** Rebasing public branches is the most common way teams lose work.

### Interactive rebase hygiene

`git rebase -i origin/main` — `pick`/`reword`/`squash`/`fixup`/`edit`/`drop`. Squash WIP noise, keep meaningful commits distinct (don't collapse a real feature into one opaque blob — you lose bisect granularity). `git commit --fixup=<sha>` + `git rebase -i --autosquash` targets fixups automatically. Abort anytime with `git rebase --abort`.

## Conflict resolution

```bash
git merge feature/new-api          # CONFLICT in src/api.rs
git status                         # list conflicted files
git diff src/api.rs                # see both sides in context (<<<<<<< ======= >>>>>>>)
# resolve by understanding BOTH intents — not blindly picking a side:
git checkout --ours   path         # keep current-branch version wholesale
git checkout --theirs path         # take incoming version wholesale
git add src/api.rs                 # mark resolved
cargo test                         # ALWAYS re-test: a clean textual merge can be a semantic conflict
git merge --continue
git merge --abort                  # bail out if it's the wrong approach
```

- ⚠ **A conflict-free merge is not a correct merge.** Two branches editing different lines can still break each other's assumptions. Run tests after every resolution.
- Document non-obvious resolutions in the merge commit body.
- ⚠ `--ours`/`--theirs` mean *opposite things* under `rebase` vs `merge` (during rebase, "ours" is the branch you're rebasing *onto*). Check which operation you're in.

### `git rerere` — resolve once, reuse forever

`git config --global rerere.enabled true`. Git records how you resolved a conflict and **replays the same resolution automatically** next time the identical conflict appears — invaluable for long-lived branches repeatedly rebased/merged, and for loom's progressive merges where the same worktree conflict can recur. Combine with periodic `git rerere diff` to sanity-check recorded resolutions.

## Worktrees

Multiple working directories sharing one `.git` — parallel branches without stashing/switching. This is the mechanism loom uses for parallel stages.

```bash
git worktree list
git worktree add ../feat-auth feature/user-auth   # new dir on a branch
git worktree add --detach ../test-v1.2 v1.2.0     # detached, for testing a tag
git worktree remove ../feat-auth                  # clean removal (must be clean, on another branch)
git worktree prune                                # after a dir was deleted by hand

# Loom: each stage runs in .worktrees/<stage-id>/ on branch loom/<stage-id>
ls .worktrees/
```

⚠ You cannot check out the same branch in two worktrees. ⚠ A `worktree remove` on a dirty tree fails — commit or discard first.

## Recovery (nothing is truly lost for ~90 days)

`git reflog` is the undo history for `HEAD` movements — the escape hatch for "I `reset --hard`'d / rebased / deleted a branch and lost commits."

```bash
git reflog                         # find the sha you were at before the mistake
git reset --hard HEAD@{2}          # jump back to that state
git branch recovered <sha>         # or resurrect a lost branch/commit
git cherry-pick <sha>              # pull back a single lost commit
```

`reset` variants: `--soft` (keep changes staged) · `--mixed`/default (keep unstaged) · `--hard` (discard — the only destructive one). `git revert <sha>` undoes a commit *with a new commit* — the safe choice on shared history (no rewrite).

## Bisect (find the commit that introduced a bug)

```bash
git bisect start && git bisect bad && git bisect good v1.2.0
# git checks out the midpoint; test it, then:
git bisect good   # or: git bisect bad
git bisect run ./test.sh   # fully automated: script exits 0=good, non-0=bad
git bisect reset
```

Bisect is only reliable if history is bisectable — every commit builds/tests. See `/loom-debugging` for using bisect within a root-cause investigation.

## Gotchas

- ⚠ `reset --hard` and `checkout -- <file>` discard uncommitted work with **no reflog entry** — it's gone. `git stash` first if unsure.
- ⚠ `git clean -fd` deletes untracked files irreversibly; always `git clean -n` (dry run) first.
- ⚠ `pull` defaults to merge, creating noise commits on feature branches. `git pull --rebase` (or set `pull.rebase = true`) keeps them linear — but only on unshared branches.
- ⚠ `cherry-pick` duplicates a commit under a new sha; cherry-picking across branches you'll later merge causes "already applied" conflicts.
- ⚠ `commit --amend` rewrites the last commit — safe only if unpushed.

## Verify before done

- [ ] No history rewrite (`rebase`/`amend`/`force`) on a shared/public branch
- [ ] Force-push, if any, is `--force-with-lease` on your own branch only
- [ ] Commits atomic, conventional-format, refactor separated from behavior change
- [ ] Tests run after every conflict resolution (textual merge ≠ semantic correctness)
- [ ] Loom: staged specific files (never `git add -A`/`.`); stayed within the worktree
- [ ] `rerere` enabled for repeated/long-lived merge work
