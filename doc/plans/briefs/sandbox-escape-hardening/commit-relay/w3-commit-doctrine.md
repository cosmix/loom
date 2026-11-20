# W3: the commit doctrine on every session surface

Stage `commit-relay`, wave 1, tier sonnet. Read `../common.md` first (decision D1). Line numbers
were read at `3fc28031` (unchanged at `2908339a`). PLAN-stage-exits-and-environment edits the
same template, skills, preamble and signal files before this plan runs: anchor every edit by
heading or symbol, never by line.

## Why

A stage worktree's git directory becomes read-only in its session (D1). Every surface a
worktree session reads must teach the exact sentence common.md fixes, and every checkout session
(a knowledge stage in the main checkout, a merge or base-conflict resolution) keeps `git add` and
`git commit`. The sentence, verbatim, on ONE line wherever it appears (the tests below match it
with `contains`):

    Commit with `loom commit -m "<type(scope): description>" -- <files>` in one Bash call, then run `loom request status <id> --wait 90` in the next call; `git add` and `git commit` fail in a stage worktree because its git directory is read-only.

## Files you own

- `loom/src/orchestrator/signals/helpers.rs`, `loom/src/orchestrator/signals/cache.rs`,
  `loom/src/orchestrator/signals/tests_commit_timing.rs`
- `CLAUDE.md.template`, `skills/loom-orchestration/SKILL.md`, `skills/loom-usage/SKILL.md`,
  `skills/loom-git-workflow/SKILL.md`, `loom-hooks/_subagent-preamble.txt`
- `loom/src/commands/stage/contracts/freeze.rs` (one string literal, section 8)

Not yours: `signals/format/helpers.rs` and `signals/format/sandbox_section.rs`
(`capsule-policy`), `loom-hooks/commit-guard.sh` (`host-git-integrity` writes its D1 message),
every other hook (W4).

## 1. Signals (`helpers.rs`, `cache.rs`)

`helpers.rs::append_commit_timing_rules(content, gate, review)` is shared by all four stable
prefixes; its last line is "Then stage your files, commit (one logical commit per concern —
module, tests, wiring, docs), and run `loom stage complete <stage-id>`." Add:

```rust
/// The D1 commit sentence every worktree-session surface carries verbatim
/// (`doc/plans/briefs/sandbox-escape-hardening/common.md`).
pub(super) const WORKTREE_COMMIT_DOCTRINE: &str = "Commit with `loom commit -m \"<type(scope): description>\" -- <files>` in one Bash call, then run `loom request status <id> --wait 90` in the next call; `git add` and `git commit` fail in a stage worktree because its git directory is read-only.";

/// How a session's commits are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitRoute {
    /// A stage worktree: through the daemon, with `loom commit`.
    Worktree,
    /// The main checkout (a knowledge stage): with `git add` and `git commit`.
    Checkout,
}
```

and a fourth parameter `route: CommitRoute`. `Checkout` keeps today's last line byte for byte.
`Worktree` writes "Then commit (one logical commit per concern — module, tests, wiring, docs)
and run `loom stage complete <stage-id>` once every commit reports applied." followed by a
space and `WORKTREE_COMMIT_DOCTRINE`, then the blank line. Update the function's doc comment.

`cache.rs` is ledgered at 524 lines (`maintainability-baseline.txt:29`), with
`generate_knowledge_distill_stable_prefix` at 74 and `generate_knowledge_stable_prefix` at 66
(`:154-155`): all three must stay exactly as long. So:

- line 3 becomes `use super::helpers::{append_commit_timing_rules, CommitRoute::{Checkout, Worktree}};`
  (one line, under 100 columns; rustfmt keeps it);
- the four calls gain `, Worktree` (standard, integration-verify, knowledge-distill: all run in a
  worktree, `cache.rs:163` "runs in worktree") or `, Checkout` (the knowledge prefix, "runs in
  main repo, no worktree", `cache.rs:239`); each stays one line.

Leave the knowledge prefix's own git lines as they are: `cache.rs:251` ("You MUST `git add
doc/loom/knowledge/` and `git commit`") and `cache.rs:281` ("Commit knowledge changes: `git add
doc/loom/knowledge/ && git commit ...`") belong to `generate_knowledge_stable_prefix`, whose
session runs in the main checkout (`stage_executor.rs:269`: only `StageType::Knowledge` does).
Also unchanged, because their sessions run in the main checkout: `signals/merge.rs:165-190` and
`signals/merge_conflict.rs:97-98`. `signals/recovery_format.rs` reuses the prefixes and needs
nothing.

Size ceilings: read the current ceilings from `signals/tests_size.rs`
(PLAN-stage-exits-and-environment's S3 raised `STABLE_PREFIX_MAX_BYTES` to 8,192) and measure
before editing; never trust a number written here. They cover `generate_stable_prefix`, the
standard-signal floor and `CLAUDE.md.template`. PLAN-stage-exits-and-environment adds its own
doctrine to the template before this plan runs. Measure the template FIRST, before any edit
(`wc -c < CLAUDE.md.template` from the repository root), compute the room left under its ceiling,
and keep your net addition to the template within that room with at least 200 bytes to spare; if
the room is smaller than section 3's text, cut your wording (the D1 sentence itself stays
verbatim) and say what you cut in your report. Your change adds about 250 bytes to each worktree
prefix. If a ceiling trips, shorten your own wording; never raise a ceiling.

## 2. Tests (`tests_commit_timing.rs`)

Its module path is `orchestrator::signals::tests::tests_commit_timing`. Add, touching no existing
assertion line:

- `worktree_prefixes_commit_through_loom_commit`: standard, integration-verify and
  knowledge-distill prefixes contain `WORKTREE_COMMIT_DOCTRINE` and not "Then stage your files".
- `the_knowledge_prefix_keeps_git_commits`: it contains "Then stage your files" and
  "git add doc/loom/knowledge/", and not "loom commit".
- `d1_commit_sentence_agrees_across_every_surface`: `CLAUDE.md.template`,
  `skills/loom-orchestration/SKILL.md`, `skills/loom-usage/SKILL.md` and
  `skills/loom-git-workflow/SKILL.md`, each through `include_str!("../../../../<path>")` as
  `tests_doctrine.rs:39-41` does, contain `WORKTREE_COMMIT_DOCTRINE`; `_subagent-preamble.txt`
  contains "loom commit".

Hazard for your report, not for you to fix: `sandbox_section_names_the_package_cache_grant` in
this file asserts `**Package-manager caches:**`, text `format/sandbox_section.rs` prints.
`capsule-policy` (D3) may reword that section; if it does, this assertion breaks at merge.

## 3. `CLAUDE.md.template`

Under `### 4. COMMIT AND COMPLETE (HOOK-ENFORCED)`: replace the opening line, the fenced block
and the "Stage `git add <specific-files>` only" line with:

    Before ending any worktree session (hard stop 3), commit, then complete. <the D1 sentence, one line>

    ```bash
    loom commit -m "feat(scope): <description>" -- <files>   # Bash call 1
    loom request status <id> --wait 90                       # Bash call 2
    loom stage complete <stage-id>                            # once every commit is applied, from the worktree root
    ```

    `loom stage complete` refuses while a commit request is not applied. A session in the main checkout (a knowledge stage, a merge or base-conflict resolution) keeps `git add <specific-files> && git commit -m "..."`, staging specific files only, never `-A` or `.`.

Keep untouched: the "**When to commit — at the END ...**" paragraph (the sentence
"Commits happen ONLY as the final step of the stage" must appear exactly once,
`tests_commit_timing.rs:103-112`), the Conventional Commits paragraph, hard stop 3, and Rule 5.
Rule 10's "Allowed: git in the current dir" stays (read-only git still works).

## 4. `skills/loom-orchestration/SKILL.md`

- `## Rule 4 — Commit and complete`: after the first paragraph add a paragraph holding the D1
  sentence, then "`loom stage complete` refuses while one of the session's commit requests is not
  applied. A checkout session (a knowledge stage, a merge or base-conflict resolution) keeps
  `git add <files>` and `git commit`."
- The COORDINATOR preamble block (the line "- NEVER run git commit, git add -A/., or loom stage
  complete - only the main agent does"): make it "- NEVER run git commit, loom commit, git add
  -A/., or loom stage complete - only the main agent does". Leave the WORKER preamble and every
  BLOCK text alone (`tests_doctrine*.rs` pin them).

## 5. `skills/loom-usage/SKILL.md`

- `## Worktree Model`: after the tree diagram and the legacy note, add a paragraph: the stage
  session commits through the daemon, then the D1 sentence.
- `## Quick Reference: Essential Commands` → `### Stage Management`: add
  `loom commit -m "..." -- <files>   # Commit from a stage worktree (daemon applies it)` and
  `loom request status <id> --wait 90   # Confirm a relayed request`.
- `#### Merge Conflict`, "Option 2: Manual resolution": the `git merge` / `git add` / `git commit`
  lines stay (the operator's own shell, outside any session); add the comment
  `# From your own shell, outside any loom session:` as the block's first line.

## 6. `skills/loom-git-workflow/SKILL.md`

- `## Commit hygiene`: replace the "⚠ Loom stages: never `git add -A` / `git add .` ..." line with
  "⚠ Loom stage worktree sessions: <D1 sentence> A checkout session (a knowledge stage, a merge
  resolution) stages specific files only — never `git add -A` / `git add .`, which stages the
  `.loom/work/` symlink."
- `## Worktrees`: the comment "# Loom: each stage runs in .worktrees/<stage-id>/ on branch
  loom/<stage-id>" gains "; its git directory is read-only there (commit with loom commit)".
- `## Verify before done`: "Loom: staged specific files (never `git add -A`/`.`); stayed within
  the worktree" becomes "Loom: a stage worktree committed with `loom commit` and confirmed with
  `loom request status --wait`; a checkout session staged specific files (never `git add -A`/`.`);
  stayed within the worktree".
- Everything else (the example `git commit -m "fix(cart): ..."`, rebase, conflict resolution,
  recovery) is generic git guidance and stays.

## 7. `loom-hooks/_subagent-preamble.txt`

In `SUBAGENT RESTRICTIONS`, the line "- NEVER run git commit - the main agent will commit your
work" becomes "- NEVER run git commit or loom commit - the main agent will commit your work".
Leave the first line exactly as it is (`spawn-guard.sh:73` matches it).

## 8. `commands/stage/contracts/freeze.rs`

The refusal text passed to `refuse(` (search `git checkout -- <path>`) tells a contract session
to revert with git. Under D1 a contract session's git directory is read-only, so `git checkout`,
`git restore` and `git stash` fail with "Unable to create '.../index.lock': Read-only file
system" (measured under bwrap). Replace the sentence "Revert every other change (`git checkout
-- <path>`, or delete the untracked file)" with:

    Revert every other change (restore a tracked file with `git show HEAD:<path> > <path>`, or delete the untracked file)

The edit stays inside the string literal and keeps its line count (the literal spans two lines
with `\` continuations: keep the space before each `\`). No test asserts the old text (checked
with `rg`).

## Sites checked and left as they are

`rg -n 'git commit|git add'` at `3fc28031`, in what you own: `cache.rs:251,281` (knowledge,
main checkout), `merge.rs`, `merge_conflict.rs`, `knowledge.rs` tests (main checkout), loom-usage
`:290-291` (operator shell), loom-git-workflow `:67,91,102,139,160` (generic git). The
`CLAUDE.md.template`, loom-orchestration and loom-git-workflow sites above are the ones you
change.

## Check

One run: `cargo test --lib orchestrator::signals::`. It includes `tests_size` and the doctrine
pins. W1 changes the relay kinds in parallel; if the crate does not compile because of a file you
do not own, skip the check and report it.
