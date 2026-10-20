# Completion Runner Blocks

> Runner-env criteria blocks

## Criteria the Confined Runner Cannot Run Ended in a Manual Block (2026-10-20)

**What happened:** stage delivery-fixes (plan token-optim/PLAN-02) blocked itself with work committed, review clean and 19 of 20 criteria green. Criterion 14 (`git ls-files '*.md' | ... | xargs bunx markdownlint-cli2`) failed inside `loom stage complete` with `fatal: not a git repository: .git/worktrees/delivery-fixes` and `ConnectionRefused downloading package manifest markdownlint-cli2`. The plan's sandbox allowed `registry.npmjs.org`. The agent had folded criterion 14 into its dispute of criterion 13; the adjudicator judged only 13 ("Criterion 14 is not the disputed criterion and is not judged here"). The next session called `loom stage block` although one criteria dispute remained. Earlier in the same stage, criterion 13 hit the fixed 300 s criterion timeout, and two hook tests failed only under loom's runner.

**Why:** the confined criteria runner denies all network whatever the plan's `network.allowed_domains` says, and it does not expose the worktree's git metadata, so any criterion using git or a package fetch cannot pass at completion. A dispute named one criterion index and the adjudicator judged only that index. Doctrine routed "a network domain or path the plan does not grant" to `loom stage block`, and the agent read an environment-caused criterion failure as that case.

**Prevention:** a criterion that fails only because of where completion runs (network, git metadata, timeout, confinement) is disputed, never blocked. Every failing criterion goes into the dispute. Plan authors check each criterion against the completion runner, not against the session sandbox.
**Fix:** operator deleted criterion 14 after running it on the host (290 files, 0 issues). Loom now closes each gap:

- **The runner** mounts linked-worktree git metadata read-only, gives Linux criteria a private `/tmp` and `/var/tmp`, tolerates a dangling resolver link, and accepts `timeout_secs` on an extended criterion.
- **`loom plan verify`** rejects a registry fetch in a confined criterion whatever `allowed_domains` says.
- **Disputes:** one dispute names several criteria, the adjudicator judges every failing one, and an accepted verdict requeues with its reasoning.
- **`loom stage block`** is refused while the latest completion shows a failing criterion.
- **The verifier** runs criteria through `loom check`.
- **The two hook tests** pass under the runner.

## Post-Merge Cleanup Raced the Closing Session and Blamed the Main Checkout (2026-10-20)

**What happened:** after delivery-fixes merged, `git worktree remove` deleted the tracked files and the `.git/worktrees/delivery-fixes` admin dir, then failed with "Directory not empty": the finished session's sandbox had recreated `.claude/.cc-writes/` three seconds earlier. Every retry, `loom worktree remove` included, then failed with "is not a working tree". The warning said "refused ... force was not set" and listed the main checkout's modified and untracked files as "Blocking paths".

**Why:** cleanup knew only `git worktree remove` and had no path for a worktree git had already dropped. `blocking_paths` (`git/cleanup/worktree.rs`) ran `git status` with the leftover directory as cwd; with no `.git` there, git walked up to the main repository. The headline assumed every failure was the dirty-tree refusal.

**Prevention:** a cleanup step that shells out to git pins repository discovery to the path it inspects. A half-removed worktree holding only scaffold is finished, not reported.

**Fix:** operator removed the empty `.claude/.cc-writes` tree, ran `git worktree prune` and `loom worktree remove delivery-fixes`, which deleted the merged branch and marked the stage merged.

## Trusting the Agent-Writable `.git` File to Pick Host Mounts (2026-10-21)

**What happened:** the first cut of the git-metadata binds followed whatever `gitdir:` the worktree's `.git` file named and bound that directory's `config`, `objects` and `refs` read-only. Review showed a crafted `.git` file or a symlinked entry would expose `~/.ssh` or `~/.kube`, and a FIFO at `commondir` would hang loom.

**Why:** the tree is agent-controlled, and the code treated a file in it as a trust anchor.

**Prevention:** a host path a sandbox exposes is derived from loom's own state (the `.worktrees/<id>` layout), and every file read on the way is bounded, no-follow and non-blocking.

**Fix:** `confine_git.rs::git_dirs` anchors on the repository that holds `.worktrees` and checks `commondir` and `gitdir` both ways. It binds nothing on any mismatch and never binds in Provision mode.

## Boundary Canaries Asserted the Write Syscall, Not the Host File (2026-10-21)

**What happened:** adding a private writable `/tmp` broke three boundary canaries and the provision test. Each expected an in-sandbox write beside a `/tmp`-hosted tree to fail; the write now succeeds into the tmpfs.

**Why:** the in-sandbox expectation stood in for the boundary property. What has to hold is that the host file stays unread and untouched.

**Prevention:** when a boundary's mounts change, re-read every canary's in-sandbox expectations. The host-side assertion is the proof; under `set -e`, a negated command (`! cmd`) asserts nothing.

**Fix:** the in-sandbox writes became `|| true`, the host-side checks stayed, and `CACHE_CANARY`'s negated lines became real checks.
