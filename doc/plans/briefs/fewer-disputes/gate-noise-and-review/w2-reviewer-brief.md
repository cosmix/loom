# gate-noise-and-review W2: `loom hook reviewer-brief`

Tier: sonnet (`loom-software-engineer`). Wave 2. Read
`doc/plans/briefs/fewer-disputes/gate-noise-and-review/common.md` first. The frozen contracts
`tests/reviewer_brief_contracts.rs` (surface below) drive the real binary; read them with
`loom stage contracts show gate-noise-and-review` before you write code.

## Files owned

`src/commands/hook/reviewer_brief.rs` (new), `src/commands/hook/reviewer_brief_plan.rs` (new),
`src/commands/hook/reviewer_brief_tests.rs` (new), `src/commands/hook/mod.rs`,
`src/commands/hook/tests_cli_surface.rs`, `src/cli/types_ops.rs` (the `HookCommands` enum),
`src/cli/dispatch.rs` (`dispatch_hook`), `src/commands/stage/review_status.rs` (visibility of
`stage_worktree_and_target`: `pub(crate)`, nothing else).
The hook scripts (`../loom-hooks/`) are W3's.

## Seams (locate by symbol)

- Pattern to follow: `src/commands/hook/worker_brief.rs`: `Config::from_env` (validates
  `LOOM_STAGE_ID` with `validate_id`, opens `LOOM_WORK_DIR` with `WorkDir::new`, loads the stage with
  `crate::verify::load_stage`), the `Envelope { nonce, brief }` printed as one JSON line, the
  `marker_nonce` check that refuses a body holding a marker line, `{}` as the fail-open output,
  `non_empty_env` from `hook/target.rs`. Do NOT copy: its stdin read (see Traps), its retrieval
  pack, its delivery record, its `max_payload_bytes` config.
- Event source: `crate::verify::integrity::{scan, load_accepted, shortfall, describe}`
  (`Scan { events, unprofiled }`, `IntegrityEvent { id, kind, language, path, base, current,
  detail, .. }`); the worktree and target branch come from
  `commands::stage::review_status::stage_worktree_and_target(work_dir, &stage)`. Pattern:
  `commands/stage/integrity_status.rs::review_integrity`.
- Plan context: `crate::verify::transitions::list_all_stages(work_dir)`; `Stage.completed_commit`,
  `Stage.stage_type` (`StageType::IntegrationVerify`), `Stage.id`, and the `consumer_deferred`
  field `criteria-authoring` added to `Stage` (`loom map --find-all consumer_deferred` for its
  exact type). The plan's source path is `[plan] source_path` in the work dir's `config.toml`
  (accessors near `base_branch()` in `src/fs/work_dir.rs`); the briefs directory follows
  `src/plan/inputs.rs` (`plan_home`, `plan_slug` from `crate::planning::slug`: the bundle is
  `<plan_home>/briefs/<plan_slug>`). Print it repo-relative.
- Registration: `HookCommands` (types_ops.rs) next to `WorkerBrief` and `ReviewerModel`;
  `dispatch_hook` (dispatch.rs); `pub mod` lines in `commands/hook/mod.rs`.

## Interface (pinned; the frozen contracts and W3 call it)

```rust
/// `loom hook reviewer-brief`: print one `{"nonce":"<32 hex>","brief":"<text>"}` line, or `{}`.
/// Never an `Err` for a missing environment, an unreadable stage or a scan failure.
pub fn emit_reviewer_brief() -> Result<()>;
```

`HookCommands::ReviewerBrief` is hidden (`#[command(hide = true)]`), doc comment "Print the
test-integrity brief for a `loom-code-reviewer` spawn (spawn guard helper)". The command reads
NO stdin.

Output contract:
- `{}` when `LOOM_STAGE_ID` or `LOOM_WORK_DIR` is unset or invalid, the stage cannot be loaded, the
  scan fails, or the stage is not integration-verify and has no integrity event.
- Otherwise one line, exactly the keys `nonce` and `brief`. `brief`'s first line is
  `<!-- loom-reviewer-brief nonce=<nonce> -->` and no other line of the body trims to a string that
  starts with `<!-- loom-`; if one does, print `{}`.

## Brief text (decided; write it exactly, tests pin these phrases)

After the marker line, in order:

1. When there is at least one event, `## Test-integrity events (computed by loom)`, then:
   `Stage `<id>` has <n> test-integrity event(s) against `<target>`. For every lost assertion
   statement below, report a finding unless a check that is equal or stronger now covers the same
   guard; name that check with file:line in the finding. Accepted events are shown for context.`
   Then per event, in scan order: `### <event.id> (<accepted | not accepted: <shortfall text>>)`,
   then by kind:
   - `AssertionEdit`: `Lost assertion statements (<k>):` and a fenced block, one statement per
     line, each `single_line`d and cut to `MAX_STATEMENT_BYTES` (400) with
     `truncate_to_byte_limit`; the fence is backticks, one more than the longest backtick run
     inside the block, at least three, info string `text`; at most `MAX_STATEMENTS_PER_EVENT` (25)
     statements, then the line `... <r> more; run loom stage review integrity <id>`.
   - `DeclTotal` / `AssertTotal`: `<describe(event)>`; add `Count events carry no statement; check
     that each removed test has a replacement.`
   - `Ratchet`: `Ratchet file loosened from base.`
2. When the stage is integration-verify (always, even with zero events):
   `## Integration-verify context (computed by loom)`, then
   `Judge the plan-wide change against what the plan asked for, not against your own expectation.`
   - `- Plan briefs: `<dir>/` (read common.md and each stage brief there).` when the directory
     exists in the worktree; omit the line otherwise.
   - `- Files the plan changed (<n>):` then up to `MAX_LISTED_FILES` (200) sorted paths, one per
     line as `  - <path>`, then `  - ... <r> more`; or `- Files the plan changed: unavailable
     (<reason>)` when `plan_changed_files` fails.
   - `- Stages that declare no consumer yet (consumer_deferred):` then `  - <stage id>: <reason>`
     per stage whose `consumer_deferred` is set, at most 50, reasons `single_line`d. Omit when none.
3. Total body cap `MAX_BRIEF_BYTES` = 16 * 1024: stop appending events at the event that would cross
   it and add `Further events omitted; run loom stage review integrity <id>.`

All paths and reasons pass through `single_line`. The integration-verify section is appended last
so the cap never drops it.

## Plan-wide changed files (`reviewer_brief_plan.rs`)

Loom records no plan base commit (`config.toml` holds only the base BRANCH name, and a stage's
`resolved_base` is a branch name). What loom records per stage is `completed_commit` (the stage tip).
`merge_stage` always writes a two-parent merge `[old target tip, stage tip]`
(`doc/loom/knowledge/architecture/merge-flow.md` step 5: "always has two parents"), and the
"fast-forward" there only moves the target ref or checkout to that merge commit. Derive the plan's
files per stage from its own change, with no new state and no plan base:

```rust
/// Union, over the plan's other completed stages, of the files each stage's merge brought to the
/// target; sorted, bounded by the caller. Stages whose merge is not found are returned in `missing`.
pub(super) struct PlanChange { pub files: Vec<String>, pub missing: Vec<String> }
pub(super) fn plan_changed_files(worktree: &Path, target: &str, stages: &[Stage], this: &str) -> Result<PlanChange>;
```

1. `commits` = `(stage id, completed_commit)` of every stage except `this` that has one. None: `Err`
   with "no stage of this plan has completed".
2. Per commit, call `crate::git::merge::control_paths::accepted_change_base(worktree, completed,
   target)` (`pub(crate)`; the pattern is `orchestrator/signals/retrieval.rs:140`; run it with the
   worktree as the repo root, `target` the merge-target branch from `stage_worktree_and_target`). Its
   `Merged { base, .. }` is `merge-base <merge>^1 <completed>`, the fork point of the stage's own
   change. `Unmerged` (the commit never reached the target's first-parent history, or reached it
   without a merge commit): the stage id goes to `missing` and contributes no file.
3. Per `Merged` stage: `control_paths::changed_paths(worktree, &base, completed)` (the same NUL-split
   `git diff --name-only -z <base> <completed>` helper).
4. `files` = the sorted, deduplicated union. A commit made on the target outside any stage never
   appears: the rule lists what the stages changed, not what `HEAD` differs by.
Surface `missing` in the brief as `- Stages not found in the target's history: <ids>` (omit when
empty). Test with a temp repo: `main` with an unrelated commit between two stage merges (its file is
absent from the result), two stage branches merged with `merge --no-ff` (one with two commits), a
stage with no `completed_commit` (skipped), and a stage commit that is only an ancestor of the target
(goes to `missing`).

## Steps

1. `reviewer_brief_plan.rs`: `plan_changed_files` and `plan_briefs_dir(work_dir, worktree) ->
   Option<String>`.
2. `reviewer_brief.rs`: environment, scan, `render(...) -> Option<String>` (pure, takes the stage,
   target, events with their status text, and the optional integration context so tests need no
   repo), `emit_reviewer_brief`. Keep each function under 50 lines; split rendering into small
   functions per section.
3. Register the command; widen `stage_worktree_and_target` to `pub(crate)`.
4. `reviewer_brief_tests.rs`: render with a lost statement; backtick run widening; 30 statements
   cut at 25 with the `... 5 more` line; a statement text of `<!-- loom-reviewer-brief nonce=aaa -->`
   keeps exactly one marker line (it sits inside a fence line that does not start with the marker,
   and the scan-level guard rejects a body line that does); no events and not integration-verify
   gives `None`; integration-verify with zero events still renders the context; the 16 KiB cap with
   the omission line and the integration section intact; `plan_changed_files` cases above; missing
   env gives `{}` (call the inner function with `None` config, not the process environment).
   `tests_cli_surface.rs`: `loom hook reviewer-brief` parses; `loom hook reviewer-brief --x` does not.
5. Run once: `cargo test --lib commands::hook::reviewer_brief`.

## Traps

- No stdin: the guard runs the command with `</dev/null`. A hook that reads stdin and exits
  early makes the writer die of SIGPIPE under `pipefail` (`v2-contracts.md` Section 2, area 3).
- The scan prints a `Note:` to stderr when the daemon is unreachable; leave stderr alone. stdout
  carries exactly one line.
- The hook runs in the stage session's environment with the worktree as its current directory;
  `stage_worktree_and_target` already resolves both.
- Statement text is agent-written and untrusted: it reaches a reviewer's prompt. One statement per
  line, fenced, cut; never put it in a heading or an HTML comment.
- 5-second bound: the shell kills the command at 5 s and fails open. Do the scan once, list files
  once, and do not shell out per file.
