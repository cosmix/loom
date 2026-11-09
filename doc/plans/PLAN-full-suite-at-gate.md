# PLAN: Full Suite Only at the Gate

Stop full test-suite runs outside the stage gate, and make repeated gate runs cheap through caching.

## Status

- **Changes 1, 2, 3, 5 and 6 are built.** Commit `d83c59b2` (2026-11-08, `fix(verification): deduplicate concurrent gate checks`, 84 files) implemented them outside loom. The pressure test of 2026-11-09 checked every brief step against the tree at `c424d0e4`: each one is present, a few trimmed or improved (see "Changes as built").
- **The four briefs in `doc/plans/briefs/full-suite-at-gate/` are a historical record. Do not execute them.** Their "current text" quotes and line numbers describe the tree before `d83c59b2`, and replaying them breaks it:
  - w4 B4.2 deletes guard lines 200-542. The guard is now 272 lines, so the delete removes the tail of `block()`, the `source` line and the walk loop, and the guard never blocks again.
  - w2 edits 1, 24 and 26 (`loom-testing:54`, `loom-java:379`, `loom-kotlin:398`): the old text is a prefix of the new, so an Edit succeeds and appends the replacement a second time.
  - w3 recreates `host_verify/pending.rs` and `store/slots.rs` with an older design than the one in the tree.
  - w1's drafts 6a and 6b overflow the `tests_size.rs` byte ceilings, and two of its acceptance commands (steps 2 and 3) fail on a correct tree.
  - Each brief opens with a banner saying this.
- **The executable work is `## Residual work` below, and only that.** It is self-contained: an executor needs this file and the tree.
- **This is a manual plan, not a loom plan.** It has no loom METADATA block, so `loom plan verify` and `loom run` reject it (`Error: No loom METADATA block found`) by design. One session outside any loom stage executes it: it spawns the R1-R3 workers named in the `## Residual work` table, owns R4 and R5 itself, and commits. The table is the file-ownership record; each item's acceptance block and `## Acceptance` are the gates. Converting it to a loom plan needs an explicit request and `/loom-plan-writer`.

## Decision

- **The waste is the full suite.** The number of targeted runs does not matter.
- **Subagents** run the tests for the code they wrote, and its module, when they are confident the code is ready or the fix is correct. They never run the whole suite.
- **Orchestrators** run the full suite only through `loom check`.
- **Integration-verify is exempt from the subagent rule.** Its orchestrator and reviewers may run the full suite. Every full run goes through `loom check integration-verify [--criterion <n>]`, so the pass cache and job joining dedupe it: on one tree the first request runs; a covered request joins it; an uncovered check from the same session waits behind it, merged with the other waiting selections; a later request hits the pass cache.
- **A repeated full check is never refused.** Caching makes it cheap.

Rejected designs and the reasons for rejecting them are in `doc/loom/knowledge/mistakes/test-sweep-and-fixture-discipline.md`, section "Duplicate Test Runs Were Framed as a Run-Count Problem".

## Evidence

Two scripts in `doc/plans/briefs/full-suite-at-gate/` scan stage transcripts under `~/.claude/projects/*worktrees*`. Run either as `python3 -I <script> <days>`.

- **`gate_audit.py`** (standard stages only; it skips `integration-verify` and `knowledge` project dirs), 14 days: 92 sessions with subagents.
  - Subagents: 878 test commands, 217 compile, 78 fmt, 19 lint. The script does not separate targeted from full runs; `full_runs.py` finds 60 full runs among them over 30 days.
  - Main agents: 215 direct test commands, 671 `loom check` calls plus 105 `loom check --criterion` calls. Most `loom check` calls are follows of one host job.
- **`full_runs.py`**: bare full-suite runner calls over 30 days, before the rollout.
  - 60 from subagents. Integration-verify sessions account for 30 (troofoo 14, loom 9, collaterai 7), standard stages for 30 (for example a `tui-tokens` worker running `cargo test --all-targets`).
  - 71 from main agents. Integration-verify orchestrators account for 50 (loom 48, troofoo 2), standard stages for 21 (`vitest run` over everything, `bun run test`, `cargo test --no-fail-fast` beside clippy and fmt, typically right beside a `loom check`).
  - The script counts attempts, including calls a guard blocked, and does not split by stage kind. Residual work R3 adds both.

## Changes as built

### 1. Doctrine text (brief `w1-doctrine.md`)

- **BLOCK-A** is one rule, in four copies that agree byte for byte: `loom-hooks/_subagent-preamble.txt`, `loom/src/orchestrator/signals/cache/blocks.rs`, `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` (`BLOCK_A`, with retired phrases built by `concat!` so the sweep cannot match them) and the guard's `block()` text in `loom-hooks/subagent-verify-guard.sh`.
- **`ORCHESTRATOR_GATE`** (`blocks.rs`) says the orchestrator never runs the full suite outside `loom check`; pinned in `tests_doctrine_gate.rs`.
- **Integration-verify**: the prefix no longer carries BLOCK-A. `INTEGRATION_VERIFY_OVERRIDE` (`cache.rs`), the preamble's EXCEPTION bullet and its "never runs the gate" bullet route full runs through `loom check integration-verify [--criterion <n>]`.
- **Review dimension 3** ("Build/test/sandbox") is spawned as `loom-software-engineer`, because `loom-code-reviewer` has no Bash. Dimension 4 ("Functional") has the same need and does not say so yet: residual R2.3.
- **Other copies** are updated: `CLAUDE.md.template`, `AGENTS.md.template`, `codex-forward.sh`, `commit-filter.sh`, README, CONTRIBUTING, `agents/loom-software-engineer.md`, `agents/loom-senior-software-engineer.md`, `skills/loom-orchestration/SKILL.md`, `skills/loom-plan-writer/references/{parallelization,codex-implementers}.md`.
- **Size limits hold.** `cache.rs` is 511 lines, equal to `loom/maintainability-baseline.txt` (`file src/orchestrator/signals/cache.rs 511`), with no slack. `tests_size.rs` leaves 35 bytes under `CLAUDE_MD_TEMPLATE_MAX_BYTES` (20 131) and 9 bytes under `ORCHESTRATION_SKILL_MAX_BYTES` (20 530). Residual work touches neither capped file.

### 2. Skills (brief `w2-skills.md`)

All 49 one-line edits across 20 skills are in the tree; adds equal deletes in every file, and frontmatter is unchanged. Each whole-suite command or "rerun until green" line is labelled as the gate's and points to the targeted form. `loom-rust` and `loom-typescript` have no such lines. Skill bodies are **embedded in the binary**: `loom/build/assets.rs` generates the `SKILLS` table with `include_bytes!`, and `loom install-assets` installs from it. No test pins skill body text. A skill edit reaches `~/.claude` only after a rebuild and install; `dev-install.sh` does both.

### 3. Parallel `loom check` in one session (brief `w3-host-jobs.md`)

Integration-verify's orchestrator and reviewers share one `LOOM_SESSION_ID`. A subagent's `loom check` relays (test `a_subagent_check_request_is_relayed_and_attributed_to_the_subagent` in `commands/hook/relay/tests.rs`).

- **Join, wait or supersede** (`host_verify/submit.rs::submit`). A covered request joins the live job whatever its session (`covers`). Residual R1.5 adds one exception: a merge resolver's check never reads the pass cache (`child_policy.rs::check_cache_policy` gives a `SessionType::Merge` job `CachePolicy::Bypass`), but as built its request carries the caller's `no_cache`, so it can join another session's cache-reading check and take that job's cached answer. An uncovered check of the same session, head and tree joins or creates the stage's waiting job (`pending.json` beside `current.json`; `host_verify/pending.rs`, `host_verify/store/slots.rs`), which runs the union of the waiting selections (`waits_behind`, `widen`). A check behind a live completion, freeze or verdict-completion job is refused (`CHECK_BEHIND_RUN`). Any other request supersedes the live job and cancels the waiting one first.
- **Cancellation.** `submit::cancel` (every takedown, retry, cleanup and `merge --resolved`) clears both slots. Stage reset (`commands/stage/state/loop_recovery/mod.rs::observed_host_jobs`) captures both live job ids under the stage lock, then cancels by id (`submit::cancel_if_id`): a replacement from another session or tree survives, and a check that waits behind a captured job is cancelled with it.
- **Promotion.** The runner promotes the waiting job on its next tick even when every runner slot is busy; starts stay gated by the slot count. Every write to either slot holds `store::lock_stage`. Shutdown and restart keep the waiting job; a waiting job on a changed tree fails fast in the child.
- **Wait state** is keyed by selection (`host_verify/wait_state.rs`), so reviewers with different selections follow their own requests; reviewers waiting together follow one waiting job.
- **`--criterion`** is sorted and deduplicated once, in `commands/verify.rs::execute` (`normalized_criteria`). The daemon keeps ticket order, which is harmless: `covers` tests containment and `union` sorts.

### 4. Finer caching: later

Two designs, with a choice between them deferred (one of them, not both):

- **Per-criterion `inputs:` globs:** narrow the content manifest in a criterion's key to the files it reads. Simple, but a missing input gives a false pass.
- **Incremental full suite:** reuse the last full pass and rerun only the tests the source graph and adapters select for the changed files, falling back to the full run when a changed file can't be traced.

### 5. Stale installed agents (brief `w4-install-and-guard.md` Part A)

`install.sh` copies no agents; it calls `loom install-assets` (`install.sh:349`, pinned by `constants_tests.rs`). `prune_stale_agents` (`loom/src/assets/install/claude.rs`) removes every `$CLAUDE_DIR/agents/loom-*.md` with no embedded source, which covers the retired `loom-verifier.md` and the renamed `loom-mechanical-engineer.md`. A regular file is first copied to `<name>.bak.<UTC stamp>` by `back_up_asset`; a symlink, dangling or not, is unlinked without being followed; a directory named `loom-*.md` is skipped. `InstallReport.pruned_agents` lists what was removed. Every install path reaches it: `install.sh`, `dev-install.sh`, `loom install-assets` and self-update. A prune error aborts `install_all` after agents are written and before commands, skills and hooks; rerunning the install recovers.

### 6. Guard split (brief `w4-install-and-guard.md` Part B)

The runner matchers moved verbatim into `loom-hooks/_verify_matchers.sh` (356 lines; its lines 15-356 equal the old guard's 200-541), sourced by `subagent-verify-guard.sh` (272 lines) after `block()`. The helper is registered in `LOOM_HOOKS` (`loom/src/fs/permissions/constants.rs`) and never as a hook, and the two fixtures that copy hook helpers (`loom/tests/integration/hooks_subagent_verify_guard.rs::install_hook`, `loom/tests/gate_dedup_contracts/support.rs`) copy it. Guard behaviour is unchanged, with one gap: when `_verify_matchers.sh` is missing, `source` fails under `set -euo pipefail` and the guard exits 1, which Claude Code treats as non-blocking, so every full-suite run passes. Residual R2.1 makes it fail closed.

### 7. Integration-verify enforcement: later

The guard exits 0 for every subagent and for the main agent of an integration-verify stage (`subagent-verify-guard.sh`, the `stage_declares standard || exit 0` / `stage_declares integration-verify` branch), so full-run routing there is doctrine only. Blocking bare suite runners there would need all of: a split of `check_command` into suite-runner blocks and lint/fmt blocks, applying only the suite blocks to integration-verify; an integration-verify wording in `block()` pointing to `loom check integration-verify`; removing the main-agent exit; flipping `subagent_in_integration_verify_stage_allowed` and adding main-agent cases; updating the preamble EXCEPTION bullet, `INTEGRATION_VERIFY_OVERRIDE` and their pin tests. Decide it from the field check (Acceptance 4): build it if bare integration-verify runs stay common next to `loom check integration-verify` calls.

## Known limits

Residual R4 records each of these in `doc/loom/knowledge/concerns/verification-v2-followups.md` ("Known Gaps"); the first two are there already.

- A waiting job's verdict covers the whole union, so a reviewer can see exit 1 for criteria it did not select; its own criteria still show a pass, and its next call hits the cache.
- Followers of one selection share one wait-state file.
- A promoted waiting job keeps the `queued_at` of its first request, and `hung_latch::job_holds` (`loom/src/orchestrator/monitor/hung_latch.rs`) times an unstarted job from `queued_at`, so time spent waiting counts against its hold.
- The `normalized_criteria` call in `commands/verify.rs::execute` is unpinned: deleting it passes every test, because relay mode exits the process.
- Integration-verify full-run routing is doctrine only (Change 7).
- The guard reads `cargo test -p <crate>` as scoped (`_verify_matchers.sh::scan_args`). In a single-crate repository such as loom that is the whole suite.

## Residual work

Every edit below is anchored by symbol and quoted text. Line numbers are hints from `c424d0e4`; when they disagree with the quote, the quote wins.

| Item | Files owned (write) | Agent |
| --- | --- | --- |
| R1 host-job fixes | `loom/src/host_verify/submit.rs` (fn `cancel` only), `loom/src/host_verify/tests_submit_pending.rs`, `loom/src/host_verify/tests_submit.rs` (one `pub(super)`), `loom/src/commands/stage/merge.rs` (fn `withdraw_or_refuse_host_job` only), `loom/src/commands/stage/merge/live_job_tests.rs`, `loom/src/commands/verify.rs` (test placement only), `loom/src/orchestrator/core/inbox_drain/check.rs` (fn `apply_check` only), `loom/src/orchestrator/core/inbox_drain/check_merge_tests.rs` | `loom-software-engineer` |
| R2 guard and doctrine text | `loom-hooks/subagent-verify-guard.sh`, `loom/tests/integration/hooks_subagent_verify_guard.rs` (and a new submodule file only if that file would pass 400 lines), `loom/src/orchestrator/signals/cache.rs` (`INTEGRATION_VERIFY_OVERRIDE` only), `loom/src/orchestrator/signals/cache/blocks.rs` (`append_review_dimension_details` only), `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs` | `loom-software-engineer` |
| R3 field-check script | `doc/plans/briefs/full-suite-at-gate/full_runs.py` | `loom-software-developer` |
| R4 knowledge | `doc/loom/knowledge/**` through the Edit tool and the `loom knowledge` CLI | executing agent, after R1 and R2 |
| R5 gate | none | executing agent, once, on the final tree |

R1, R2 and R3 write disjoint files and run in parallel. Workers run only the narrow acceptance commands listed under their item, never the full suite.

### R1. Host-job fixes

**R1.1 `submit::cancel` reads the slots in promotion-safe order** (`loom/src/host_verify/submit.rs`, fn `cancel`). Promotion (`pending::promote_locked`) saves the waiting job as current, then clears pending. `cancel`'s lock-free pre-check reads current, then pending, so a cancel interleaved with a promotion can see current terminal and pending already cleared, return `None`, and leave the promoted job live. Callers that run outside the daemon (`commands/stage/skip_retry.rs`, `git/cleanup/batch.rs`) can hit this. Replace:

```rust
    let live = |record: &JobRecord| !record.state.is_terminal();
    if !store::load_current(work_dir, stage_id)?.is_some_and(|record| live(&record))
        && store::load_pending(work_dir, stage_id)?.is_none()
    {
        return Ok(None);
    }
```

with:

```rust
    let live = |record: &JobRecord| !record.state.is_terminal();
    // Read pending BEFORE current: promotion saves the waiting job as current
    // and then clears pending, so this order always sees a job moving between
    // the slots, while the reverse order can miss it and leave it live.
    if store::load_pending(work_dir, stage_id)?.is_none()
        && !store::load_current(work_dir, stage_id)?.is_some_and(|record| live(&record))
    {
        return Ok(None);
    }
```

No regression test: the race needs two processes interleaving between two file reads, and the module has no injection seam for it. The comment carries the invariant.

**R1.2 `merge --resolved` treats a live waiting job as busy** (`loom/src/commands/stage/merge.rs`, fn `withdraw_or_refuse_host_job`). Runner shutdown cancels current ("daemon stopped") and leaves the waiting job queued for the next daemon start. The function reads only `current.json`, so `merge --resolved` proceeds and the next daemon start runs the waiting job in the worktree. Replace:

```rust
    let Some(job) = crate::host_verify::store::load_current(work_dir, stage_id)?
        .filter(|job| !job.state.is_terminal())
    else {
        return Ok(());
    };
```

with:

```rust
    // A daemon shutdown cancels current and leaves its waiting job queued for
    // the next start, so a live waiting job keeps the worktree as busy as a
    // live current one. `submit::cancel` below withdraws both slots. Read
    // pending BEFORE current, as `submit::cancel` does: promotion saves the
    // waiting job as current and then clears pending, so this order sees a
    // job moving between the slots.
    let live = |job: &crate::host_verify::JobRecord| !job.state.is_terminal();
    let waiting = crate::host_verify::store::load_pending(work_dir, stage_id)?.filter(live);
    let current = crate::host_verify::store::load_current(work_dir, stage_id)?.filter(live);
    let Some(job) = current.or(waiting) else {
        return Ok(());
    };
```

If the function's doc comment names only the current job, add "or waiting" to it. The rest of the function stays: a waiting job is never `Running`, so after `submit::cancel` it takes the "resolved proceeds" path. Add to `loom/src/commands/stage/merge/live_job_tests.rs`, beside `a_running_daemon_refuses_and_leaves_the_job_alone`:

```rust
#[test]
#[serial]
fn a_waiting_check_behind_an_ended_one_refuses_then_is_withdrawn() {
    let temp = TempDir::new().unwrap();
    let mut current = stage_with_a_queued_check(temp.path());
    let work_dir = temp.path().join(".loom").join("work");
    let (head, tree) = ("a".repeat(40), "b".repeat(64));
    let uncached = SubmitRequest {
        stage_id: STAGE,
        session_id: "resolver",
        request_id: "r2",
        kind: JobKind::Check,
        head: &head,
        tree_digest: &tree,
        no_cache: true,
        criteria: None,
    };
    let waiting = match submit(&work_dir, &uncached, Utc::now()).unwrap() {
        Submission::Waiting { pending, .. } => *pending,
        other => panic!("expected a waiting check, got {other:?}"),
    };
    current.state = JobState::Cancelled;
    save_current(&work_dir, &current).unwrap();

    let error = format!(
        "{:#}",
        withdraw_or_refuse_host_job(STAGE, &work_dir, true).unwrap_err()
    );
    assert!(error.contains(&waiting.id), "{error}");
    withdraw_or_refuse_host_job(STAGE, &work_dir, false).unwrap();
    assert_eq!(
        crate::host_verify::store::load_pending(&work_dir, STAGE).unwrap(),
        None
    );
}
```

The uncached full request is not covered by the cached full job `stage_with_a_queued_check` queues, so it waits (same session, head and tree). Read `stage_with_a_queued_check` first: the test assumes its job uses session `"resolver"`, head `"a".repeat(40)` and tree `"b".repeat(64)`; take whatever values it uses. The two calls cover both admission paths: `true` (a daemon is running) refuses and names the waiting job; `false` (no daemon) withdraws it. The promotion interleaving between the two reads has no injection seam, as in R1.1; the read order and its comment carry it. Regression proof: with R1.2's code reverted and the test kept, the test fails on `unwrap_err()`.

**R1.3 two missing submit tests** (`loom/src/host_verify/tests_submit_pending.rs`; helpers `state_root`, `subset`, `request`, `queued`, `waiting`, `superseded`, `HEAD_A` come from `tests_submit.rs` through `use super::tests_submit::*`; `pair` and `assert_cancelled` are local). No existing test changes the tree digest while keeping the head, so dropping the tree comparison from `waits_behind` passes every test. No test sends a narrower or cached request after a full uncached waiting one, so a last-wins `no_cache` or a narrowing `union` passes every test. Add:

```rust
#[test]
fn a_changed_tree_on_the_same_head_supersedes_the_check_and_its_waiting_one() {
    let (_tmp, work) = state_root();
    let (mut current, waiting) = pair(&work);
    current.state = JobState::Running;
    store::save_current(&work, &current).unwrap();
    let digest = "d".repeat(64);
    let edited = SubmitRequest {
        tree_digest: &digest,
        ..subset("r3", &[3])
    };
    let (_, next) = superseded(submit(&work, &edited, Utc::now()).unwrap());
    let reason = format!("superseded by {}", next.id);
    assert_cancelled(&work, &current, &reason);
    assert_cancelled(&work, &waiting, &reason);
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
    assert_eq!(store::load_current(&work, STAGE).unwrap(), Some(next));
}

#[test]
fn widening_never_narrows_a_full_waiting_job_or_drops_its_no_cache() {
    let (_tmp, work) = state_root();
    let current = queued(submit(&work, &subset("r1", &[1]), Utc::now()).unwrap());
    let (full, behind) = waiting(submit(&work, &request("r2", HEAD_A, true), Utc::now()).unwrap());
    assert_eq!(behind, current.id);
    assert_eq!((full.criteria.clone(), full.no_cache), (None, true));
    let (widened, _) = waiting(submit(&work, &subset("r3", &[4]), Utc::now()).unwrap());
    assert_eq!(widened.id, full.id);
    assert_eq!((widened.criteria, widened.no_cache), (None, true));
}
```

Read `request` and `subset` in `tests_submit.rs` first: both tests assume they share `STAGE`, the session, `DIGEST`, and that `subset` uses `HEAD_A`. Adjust the head argument if `subset` uses another.

**R1.4 test placement.** In `loom/src/commands/verify.rs` the `#[test] fn criterion_selections_are_sorted_and_deduplicated` sits after `mod tests`, at module level; move it inside `mod tests` (reach the function through the module's `use super::...`). In `loom/src/host_verify/tests_submit.rs`, drop `pub(super)` from `#[test] fn joined_requests_append_and_keep_the_last_sixteen` after `rg -n joined_requests_append_and_keep_the_last_sixteen loom/src` shows no other reference.

**R1.5 a merge resolver's check counts as uncached** (`loom/src/orchestrator/core/inbox_drain/check.rs`, fn `apply_check`). `covers` lets a cached request join any live job of the same head, tree and selection, whatever its session. A resolver's job never reads the pass cache (`child_policy.rs::check_cache_policy`, keyed on the job record's session), but a resolver request with `no_cache: false` joins an ordinary session's cache-reading check and takes its answer. Counting the resolver's request as uncached closes this in the existing `covers` rule (`current.no_cache || !request.no_cache`): it joins only a job that reads no cache, and otherwise supersedes a `Check` (cross-session, so `waits_behind` is false) or is refused with `CHECK_BEHIND_RUN` behind a `Complete`, freeze or verdict-completion job. An ordinary request joining a resolver's job gets a run that read no cache, which is safe. Judges keep their own policy: `check_cache_policy` special-cases only `SessionType::Merge`. In the `SubmitRequest` literal inside `apply_check`, replace:

```rust
            no_cache: request.no_cache,
```

with:

```rust
            // A resolver's job never reads the pass cache (child_policy.rs), so
            // its request must not join a job that does.
            no_cache: request.no_cache || record.session_type == SessionType::Merge,
```

`SessionType` is already imported there (`admit_session` matches on it); `check.rs` goes from 385 to 387 lines. Add to `loom/src/orchestrator/core/inbox_drain/check_merge_tests.rs`, using its `resolver`, `relay_check` and the imported `repository`, `fixture`, `STAGE`:

```rust
#[test]
fn a_resolver_supersedes_a_cached_check_of_another_session_instead_of_joining_it() {
    let fx = fixture();
    let head = repository(&fx);
    let record = resolver(&fx, SessionStatus::Running, "loom/s1");
    fx.stage(StageStatus::MergeConflict, None);
    let tree = "0".repeat(64);
    let ordinary = crate::host_verify::submit::SubmitRequest {
        stage_id: STAGE,
        session_id: "stage-session",
        request_id: "r1",
        kind: JobKind::Check,
        head: &head,
        tree_digest: &tree,
        no_cache: false,
        criteria: None,
    };
    let crate::host_verify::submit::Submission::Queued(cached) =
        crate::host_verify::submit::submit(&fx.work_dir, &ordinary, Utc::now()).unwrap()
    else {
        panic!("expected the ordinary check to queue");
    };

    let settled = relay_check(&fx, &record, &head);

    assert!(matches!(settled, Settle::Applied(Some(_))), "{settled:?}");
    let job = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_ne!(job.id, cached.id);
    assert_eq!((job.session_id.as_str(), job.no_cache), (record.id.as_str(), true));
}
```

The tree is `"0".repeat(64)` because `request(head)` in `check.rs`'s test module sends that digest. Check the `SubmitRequest` and `Submission` paths with `loom map --find-all SubmitRequest` and use the re-export `check.rs` imports if there is one. Regression proof: without R1.5's change the resolver's request joins, `load_current` returns the ordinary job, and `assert_ne!` fails. The existing `the_resolver_of_this_stage_is_queued_in_either_merge_status` still passes: it asserts kind, state and session only.

**R1 acceptance**, from `loom/`. Each test command writes a full log and prints Cargo's own exit status; a pipe into `tail` would report `tail`'s status instead (loom runs shell criteria under `sh -c` without `pipefail`):

```bash
LOG="${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}"
cargo test --lib host_verify:: > "$LOG/r1-host.log" 2>&1; echo "exit=$?"
cargo test --lib live_job_tests > "$LOG/r1-merge.log" 2>&1; echo "exit=$?"
cargo test --lib commands::verify > "$LOG/r1-verify.log" 2>&1; echo "exit=$?"
cargo test --lib inbox_drain::check > "$LOG/r1-inbox.log" 2>&1; echo "exit=$?"
rg -n -m 20 -M 300 'FAILED|panicked|^error|test result' "$LOG"/r1-*.log
wc -l src/host_verify/submit.rs src/host_verify/tests_submit_pending.rs src/commands/stage/merge.rs src/commands/stage/merge/live_job_tests.rs src/orchestrator/core/inbox_drain/check.rs src/orchestrator/core/inbox_drain/check_merge_tests.rs
```

Each test command prints `exit=0`, and each log's `test result` line shows zero failed and at least one passed; every file stays at or under 400 lines.

### R2. Guard and doctrine text

**R2.1 the guard fails closed when `_verify_matchers.sh` is missing** (`loom-hooks/subagent-verify-guard.sh`). The `source` sits after role detection and the stage lookup, so ordinary sessions (the `live_loom_session || exit 0` line) and integration-verify stages exit 0 before reaching it; failing closed blocks Bash only in live sessions the guard actually polices. Replace:

```bash
# shellcheck source=_verify_matchers.sh
source "$(dirname "${BASH_SOURCE[0]}")/_verify_matchers.sh"
```

with:

```bash
# shellcheck source=_verify_matchers.sh
source "$(dirname "${BASH_SOURCE[0]}")/_verify_matchers.sh" || {
	echo "subagent-verify-guard.sh: _verify_matchers.sh is missing or unreadable beside this hook - blocking (fail closed). Reinstall the hooks with \`loom install-assets\`." >&2
	exit 2
}
```

In the header, change `(also jq not` / `# installed - fail closed)` to `(also jq not` / `# installed or _verify_matchers.sh missing - fail closed)`, keeping two lines. Add the test `a_missing_matchers_helper_fails_closed` to `loom/tests/integration/hooks_subagent_verify_guard.rs`, modelled on `check_subagent_case`: same temp-dir and stage setup, `install_hook(dir)`, then `std::fs::remove_file(dir.join("_verify_matchers.sh"))`, then run `cargo test` as a subagent through `run_hook_in_tree`; return early through `skip_unless_gate_visible` like the other process-tree tests; assert exit code 2 and that stderr contains `_verify_matchers.sh`. The file is 376 lines: if the test would push it past 400, put it in a new `hooks_subagent_verify_guard_helper.rs` declared the way `mod main;` declares `hooks_subagent_verify_guard_main.rs`. Regression proof: without the `|| { ... }` the guard exits 1 and the test fails.

**R2.2 integration-verify override wording** (`loom/src/orchestrator/signals/cache.rs`, `INTEGRATION_VERIFY_OVERRIDE`). Two text changes inside the existing source lines (lines may grow; add none, `cache.rs` must stay 511 lines):

- `cargo clippy -- -D warnings` becomes `cargo clippy --all-targets -- -D warnings` (CONTRIBUTING requires `--all-targets`; without it clippy skips test code). In `tests_doctrine_prefixes.rs`, change `assert!(prefix.contains("cargo clippy -- -D warnings"));` to the same new string.
- The sentence `On one tree the first request runs; covered requests join it; later requests hit the pass cache.` (split across two source lines by `\` continuations) becomes `On one tree the first request runs; a covered request joins it; an uncovered one from the same session waits and runs with the other waiting selections, so its verdict can list criteria it did not select; later requests hit the pass cache.` Add the needle `"its verdict can list criteria it did not select"` to the "IV ownership doctrine lost" needle list in `tests_doctrine_prefixes.rs` (the list that contains `"stale evidence never waives a failing gate"`).

The integration-verify prefix has no `tests_size.rs` byte ceiling (`STABLE_PREFIX_MAX_BYTES` measures the standard prefix).

**R2.3 review dimension 4 names its agent** (`loom/src/orchestrator/signals/cache/blocks.rs`, `append_review_dimension_details`). Replace this Rust string literal:

```text
"4. **Functional** — RUN the feature end-to-end on realistic inputs; confirm the output is correct and the feature is registered, mounted, and callable.\n"
```

with:

```text
"4. **Functional** — spawn this one as `loom-software-engineer` too, since it runs commands: RUN the feature end-to-end on realistic inputs; confirm the output is correct and the feature is registered, mounted, and callable.\n"
```

In `tests_doctrine_prefixes.rs`, add this needle to the "IV prefix lost review guard" list, after the Build/test/sandbox needle:

```text
"**Functional** — spawn this one as `loom-software-engineer`",
```

**R2.4 stale test-module comment** (`loom/tests/integration/hooks_subagent_verify_guard.rs`, module doc, lines 8-11). Replace:

```text
//! its own gate (its orchestrator runs `loom check`; its reviewers run only
//! targeted checks, which the subagent preamble states and this hook does not
//! enforce); git commits are never blocked by this hook even
```

with:

```text
//! its own gate (its orchestrator and reviewers run the full suite only through
//! `loom check integration-verify`, which the subagent preamble states and this
//! hook does not enforce); git commits are never blocked by this hook even
```

**R2 acceptance.** From the repo root, then from `loom/`:

```bash
bash -n loom-hooks/subagent-verify-guard.sh
shellcheck -x -P SCRIPTDIR loom-hooks/subagent-verify-guard.sh loom-hooks/_verify_matchers.sh
./scripts/check-hook-syntax.sh
cd loom
LOG="${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}"
NC_TMP="${LOOM_SCRATCH_DIR:-/var/tmp}/nc-tmp"; mkdir -p "$NC_TMP"
case "$NC_TMP" in *[cC][lL][aA][uU][dD][eE]*) NC_TMP=/var/tmp/loom-nc-tmp; mkdir -p "$NC_TMP" ;; esac
TMPDIR="$NC_TMP" cargo test --test integration hooks_subagent_verify_guard > "$LOG/r2-guard.log" 2>&1; echo "exit=$?"
cargo test --test gate_dedup_contracts > "$LOG/r2-dedup.log" 2>&1; echo "exit=$?"
cargo test --test hook_false_positive_contracts > "$LOG/r2-fp.log" 2>&1; echo "exit=$?"
cargo test --lib orchestrator::signals:: > "$LOG/r2-signals.log" 2>&1; echo "exit=$?"
rg -n -m 20 -M 300 'FAILED|panicked|^error|SKIP|test result' "$LOG"/r2-*.log
wc -l src/orchestrator/signals/cache.rs
```

Each test command prints `exit=0` (never pipe one into `tail`: the pipe reports `tail`'s status), and each log's `test result` line shows zero failed and at least one passed. `shellcheck` without `-P SCRIPTDIR` reports SC1091 from the repo root; that is expected. The integration suite asserts its temp dir has no "claude" in its path (`temp_dir_no_claude`), hence `NC_TMP`. A "SKIP" line in its output means the sandbox hides the process tree; report it, never count it as a pass. `wc -l` prints 511.

### R3. Field-check script

Extend `doc/plans/briefs/full-suite-at-gate/full_runs.py`; keep its `FULL` and `SCOPED_FLAGS` regexes and the `<days>` argument.

1. Optional `--since YYYY-MM-DD` (UTC midnight) overrides `<days>` for the mtime cutoff.
2. Classify each project dir as `iv` when its name contains `integration-verify`, `knowledge` when it contains `knowledge`, else `standard` (the rule `gate_audit.py` uses to skip dirs).
3. Pair each hit with its result. In entries with `type == "user"`, `message.content[]` items with `type == "tool_result"` carry `tool_use_id` and `content` (a string or a list of `{"type": "text", "text": ...}`). A hit whose result text contains `[subagent-verify-guard/` counts as `blocked`, else `ran`.
4. Print one count per `(repo, kind, role, outcome)`, then the examples as now, each prefixed with its kind and outcome.

Acceptance: `python3 -I doc/plans/briefs/full-suite-at-gate/full_runs.py 30` exits 0, and its counts summed over every key equal the totals the unmodified script prints for the same `<days>` when run back to back (`git stash` is not needed: copy the old script to `$TMPDIR` first). `python3 -I doc/plans/briefs/full-suite-at-gate/full_runs.py 1 --since 2026-11-09` exits 0.

### R4. Knowledge (after R1 and R2)

Edit with the Edit tool, then run `loom knowledge sync` and `loom knowledge check`. State current truth only. Each block below is literal text: the first line of a pair is the text to find, the second its replacement.

**K1** `doc/loom/knowledge/architecture/verification-v2-gates.md`, first section (`## Verification v2 Gates: Order, Owners, and What Each Gate Reads`):

```text
Concurrent matching requests join the same host job; matching completed passes use the cache, and callers wait for the result.
A covered request joins the live host job, an uncovered check from the same session waits behind it in the pending slot, completed passes are reused from the cache, and callers wait for the result.
```

**K2** same file, `## Job Records and State`:

```text
A check on the same session, head and tree as a live check joins it when covered; an uncovered request waits in the pending slot.
A covered check joins the live job whatever its session (`submit.rs::covers`); a merge resolver's request counts as uncached (`inbox_drain/check.rs::apply_check`), so it joins only a job that reads no pass cache; an uncovered check of the same session, head and tree waits in the pending slot (`submit.rs::waits_behind`); a check behind a live completion, freeze or verdict-completion job is refused; any other request supersedes the live job and cancels the waiting one.
```

**K3** same file, `## Cancellation and Restart`:

```text
Promotion during reset is covered, and jobs submitted afterward are preserved.
Promotion during reset is covered. A replacement from another session or tree submitted afterward survives; a check that waits behind a captured job afterward is cancelled with it, because `submit::cancel_if_id` cancels the waiting slot when the captured current matches. `merge --resolved` treats a live waiting job as busy.
```

**K4** `doc/loom/knowledge/entry-points/hooks.md`, `## Hook Scripts — What Each Does`, the `subagent-verify-guard.sh` row:

```text
**Still regexes raw strings** (last hook not converted; see concerns.md).
Walks command-position tokens with a private scanner (see concerns.md); the runner matchers live in the sourced `_verify_matchers.sh`, registered in `LOOM_HOOKS` but never as a hook; a missing helper fails closed.
```

**K5** `doc/loom/knowledge/architecture/hook-system.md`, `## Subagent Isolation`, the `subagent-verify-guard.sh` row:

```text
**still matches raw command strings** (see concerns.md).
walks command-position tokens with a private scanner (see concerns.md); its runner matchers live in the sourced `_verify_matchers.sh`, and a missing helper fails closed.
```

**K6** `doc/loom/knowledge/entry-points/cli-and-plan-pipeline.md`, append to the end of the bullet that starts ``- `loom install-assets` ``:

```text
 `install_files` also runs `prune_stale_agents` (`assets/install/claude.rs`), removing each `$CLAUDE_DIR/agents/loom-*.md` with no embedded source: a regular file is first copied to `<name>.bak.<UTC stamp>`, a symlink is unlinked unfollowed, a directory is skipped. A prune error aborts `install_all` before commands, skills and hooks; rerun the install.
```

**K7** `doc/loom/knowledge/concerns/verification-v2-followups.md`, insert after the bullet that starts `- **Followers of one selection share wait state.**`:

```text
- **A promoted waiting job's hold counts its wait.** `hung_latch::job_holds` (`orchestrator/monitor/hung_latch.rs`) times an unstarted job from `queued_at`, and a waiting job keeps the `queued_at` of its first request. Following sessions heartbeat on every `loom check`. Revisit if a waiting job's session is flagged hung while the check ahead of it ran.
- **The `--criterion` normaliser call is unpinned.** Deleting `normalized_criteria` from `commands/verify.rs::execute` passes every test, because relay mode exits the process. Revisit when the relay path gains an in-process test seam.
- **Integration-verify full runs are routed by doctrine only.** `subagent-verify-guard.sh` exits 0 for every subagent and the main agent of an integration-verify stage. `doc/plans/briefs/full-suite-at-gate/full_runs.py` measures bare runs there. Revisit if they stay common next to `loom check integration-verify` calls.
- **`cargo test -p <crate>` passes the guard as scoped.** `_verify_matchers.sh::scan_args` reads `-p`/`--package` as scoped; in a single-crate repository such as loom that is the whole suite. Revisit if field data shows `-p` used as a full run.
```

Acceptance: `loom knowledge check` passes; `rg -n '_verify_matchers' doc/loom/knowledge/entry-points/hooks.md doc/loom/knowledge/architecture/hook-system.md` finds both rows; `rg -n 'Still regexes raw strings|still matches raw command strings|jobs submitted afterward are preserved|Concurrent matching requests join' doc/loom/knowledge; echo "rg exit=$?"` prints `rg exit=1`.

### R5. Gate

Run once on the final tree, from an ordinary session outside any loom stage (the guard blocks a standard stage's main agent from an unfiltered suite). These are the repo's canonical gates (CI `.github/workflows/ci.yml`; `doc/loom/knowledge/conventions/git-and-build-workflow.md`). From `loom/`, with `NC_TMP` set as in R2:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
TMPDIR="$NC_TMP" cargo test --all-targets --no-fail-fast -- --test-threads=4
cargo test --doc --workspace --all-features -- --test-threads=1
cargo test --test maintainability
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
cargo audit
../scripts/flake-check.sh
```

From the repo root:

```bash
./scripts/check-hook-syntax.sh
LOOM_HOOK_TEST_REQUIRE_BINARY=1 bash loom-hooks/tests/run-all.sh
git ls-files '*.md' | grep -v '^doc/plans/' | grep -v '^loom/tests/fixtures/' | xargs bunx markdownlint-cli2
```

This is the `CONTRIBUTING.md` "The Gate" list plus the hook suites; the web check is out (R1-R4 touch nothing under `web/`). Record each command's exit status. `cargo audit` fetches the advisory database: if it cannot (no network, sandbox denial), report it as unverified, never as passed. Every other command must exit 0. `run-all.sh` takes about three minutes. On a failure, fix it and rerun only the failed target (`cargo test --test <target> <filter>` or the one hook script), then rerun the full gate once before committing. Commit one concern per commit: `fix(verification): ...` for R1, `fix(hooks): ...` for R2, `chore(plans): ...` for R3, `docs(knowledge): ...` for R4.

## Cross-plan notes

No sibling has to build anything for the residual work.

**Baseline and ordering (blocking for the siblings named).** `d83c59b2` plus this plan's residual work is the baseline every sibling below builds on. While the residual work runs, no sibling stage may write a file in its ownership table; a sibling that has started on one of those files finishes first, and the residual worker rebases onto it. Required sibling amendments, to be written into those plans before they run concurrently with or after this one:

- **`PLAN-live-test-lane` live-checks R1** (owns `host_verify/{mod,child,submit,apply}.rs`, `commands/verify.rs`, `inbox_drain/check.rs`) and **`PLAN-autonomous-recovery` stall-recovery W2** (owns `src/host_verify/**`): the new `Live` and `Land` arms keep `Submission::Waiting`, the `pending::cancel_locked` call in `submit`'s supersede branch, `commands/verify.rs::normalized_criteria`, and R1.5's `no_cache: request.no_cache || record.session_type == SessionType::Merge` in `apply_check` (live-test-lane R1 rewrites `admit_session` and adds `job_kind_for` there). Each adds a test where its new kind supersedes a live check that has a waiting job, asserting both are cancelled and `pending.json` is cleared.
- **`PLAN-loom-ui-mods` loom-ui W3** (owns `assets/install.rs`, `assets/install/claude.rs`, `commands/install_assets.rs`): keeps `prune_stale_agents` and the `InstallReport.pruned_agents` field that `install_all` reports.
- **`PLAN-model-router-hooks` wiring** (owns `fs/permissions/constants.rs`): keeps `_verify_matchers.sh` in `LOOM_HOOKS` and out of every hook table.
- **`PLAN-web-host-graft-followthrough` implement-host-and-context M/C** (signal cache, `signals/tests_doctrine*.rs`, `maintainability-baseline.txt`): keeps BLOCK-A byte-identical across its four copies and `cache.rs` at or under its 511-line baseline, which `CONTRIBUTING.md` forbids raising.
- **Knowledge curation** (all of the above and `PLAN-planner-background-activity-and-skills`): R4 edits `verification-v2-gates.md`, `hooks.md`, `hook-system.md`, `cli-and-plan-pipeline.md` and `verification-v2-followups.md`; a sibling's knowledge stage running at the same time edits those files only after R4 commits.

These sibling defects were found while checking shared seams; each belongs to that sibling's plan, and the first three block it:

- **`PLAN-autonomous-recovery`**: gate-waivers W4 (`briefs/autonomous-recovery/gate-waivers/w4-doctrine.md`) adds about 239 bytes to `CLAUDE.md.template`, which has 35 bytes of headroom (`tests_size.rs`, `CLAUDE_MD_TEMPLATE_MAX_BYTES`); check-routes W5 and gate-waivers W4 add text to `skills/loom-orchestration/SKILL.md`, which has 9 bytes (`ORCHESTRATION_SKILL_MAX_BYTES`). Both must trim to fit; the ceilings say not to raise them. `d83c59b2` shrank both files, so this plan did not consume that headroom.
- **`PLAN-live-test-lane`**: `briefs/live-test-lane/live-profile/s1-schema.md` says `host_verify/runner.rs` is 387 lines; it is 400, the limit, and the exhaustive `JobKind` match in `budget_for_job` needs new arms. R1 of the live-checks stage adds to `orchestrator/core/inbox_drain/check.rs`, now 385 lines.
- **`PLAN-model-router-hooks`**: its wiring stage registers new hooks but omits `loom/src/fs/permissions/tests/hooks_tests.rs` from `files:`; that file asserts `pre_tool.len() == 49` and `prompt.len() == 2`.
- **Live and Land job kinds** (`PLAN-live-test-lane` `r1-live-runs.md`, `PLAN-autonomous-recovery` stall-recovery `w2-land-job.md`): a Live or Land request supersedes a current check and cancels its waiting job (`submit.rs` supersede branch), and a check behind it is refused with `CHECK_BEHIND_RUN` ("a completion or contract run is in progress"), which misnames the job. Those briefs should say so and reword the refusal or add a reason per kind.
- **`PLAN-loom-ui-mods`**: `PLAN-loom-ui-mods.md:160` and `briefs/loom-ui-mods/loom-ui/common.md:125` use the retired `loom-verifier`; the gate is the main agent's `loom check <stage-id>`. Its W3 (`w3-installer.md`) owns `assets/install.rs`, `assets/install/claude.rs` and `commands/install_assets.rs`, which `d83c59b2` changed for pruning; its line anchors there and in `install.sh` (364, 370 lines; now 349, 355) are stale, and it must not add a second `install-assets` line to `install.sh` (`constants_tests.rs` pins one).
- **Integration-verify descriptions with a bare `cargo test`**: `PLAN-model-router-hooks.md` (around lines 143 and 418-445), `PLAN-loom-ui-mods.md` (around 334) and `PLAN-web-host-graft-followthrough.md` (around 344 and 373) should say "through `loom check integration-verify`".
- **Shared files with the residual work**: `submit.rs` (R1.1 touches only fn `cancel`; the Live and Land work changes `submit` and its match arms), `inbox_drain/check.rs` (R1.5 touches one field in `apply_check`; live-test-lane R1 rewrites `admit_session` and adds `job_kind_for`) and `cache.rs` (R2.2 touches only `INTEGRATION_VERIFY_OVERRIDE`; `PLAN-web-host-graft-followthrough` worker M owns `cache.rs` and states that plan must not run as written). Whichever merges second rebases.

## Out of scope

- **Follow calls of a running job.** The repeated `loom check` lines are one host job being followed. `host_verify/wait.rs::budget_for` caps a call at 100 s to stay inside Claude Code's 120 s Bash limit.
- **Any count of targeted runs**, and any refusal of a full `loom check`.
- **Rewriting the briefs.** They stay as the record of `d83c59b2`.

## Acceptance

1. R1-R4 acceptance commands pass as stated.
2. `rg -n 'AT MOST ONE narrowly|narrowly-scoped check|narrowly scoped check|ONE scoped check|skip if unsure|reviewers run only targeted' --glob '!doc/**' --glob '!target'; echo "rg exit=$?"` prints only `rg exit=1`. The doctrine tests' retired phrases are built with `concat!`, so they do not match.
3. The R5 gate passes on the final tree.
4. **Field check (observational).** Owner: the operator, after the first loom plan run once `dev-install.sh` has installed a binary and hooks built from `d83c59b2` or later. Run `python3 -I doc/plans/briefs/full-suite-at-gate/full_runs.py <days> --since <install date>` (R3).
   - Standard stages: zero `ran` bare full-suite runs by subagents and main agents. Baseline, 30 days before the rollout: 30 subagent and 21 main hits.
   - Integration-verify: report `ran` bare runs next to the count of `loom check integration-verify` calls. There is no zero target; this ratio decides Change 7. Baseline: 30 subagent and 50 main hits.

## Pressure tests

- 2026-11-09: Claude opus, Codex gpt-6-astra; 1 round, review addressed
