# PLAN: Full Suite Only at the Gate

Stop full test-suite runs outside the stage gate, and make repeated gate runs cheap through caching.

## Status

- **Built** in `d83c59b2` (2026-11-08) and checked against `c424d0e4` on 2026-11-09: the doctrine text, the skill edits, parallel `loom check` in one session (join, wait or supersede through the pending slot), stale-agent pruning at install, and the guard's runner matchers split into `loom-hooks/_verify_matchers.sh`.
- **Do not execute the briefs** in `doc/plans/briefs/full-suite-at-gate/`. They record `d83c59b2`; their quotes predate it, and replaying them breaks the tree. Its two scripts, `gate_audit.py` and `full_runs.py`, stay in use.
- **Manual plan.** It has no loom METADATA block, so `loom run` rejects it. One session outside any loom stage executes `## Residual work`: it spawns the R1-R3 workers named in the table, owns R4 and R5, and commits.

## Decision

- **The waste is the full suite.** The number of targeted runs does not matter.
- **Subagents** run the tests for the code they wrote, and its module, when they are confident the code is ready or the fix is correct. They never run the whole suite.
- **Orchestrators** run the full suite only through `loom check`.
- **Integration-verify is exempt from the subagent rule.** Its orchestrator and reviewers may run the full suite. Every full run goes through `loom check integration-verify [--criterion <n>]`, so the pass cache and job joining dedupe it: on one tree the first request runs; a covered request joins it; an uncovered check from the same session waits behind it, merged with the other waiting selections; a later request hits the pass cache.
- **A repeated full check is never refused.** Caching makes it cheap.

Rejected designs and the reasons for rejecting them are in `doc/loom/knowledge/mistakes/test-sweep-and-fixture-discipline.md`, section "Duplicate Test Runs Were Framed as a Run-Count Problem".

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

## Ordering with sibling plans

While the residual work runs, no other stage writes a file in its ownership table; whichever change merges second rebases. Sibling plans that touch these files keep what `d83c59b2` and this work add:

- `PLAN-live-test-lane`, `PLAN-autonomous-recovery` (`host_verify/**`, `commands/verify.rs`, `inbox_drain/check.rs`): `Submission::Waiting`, the `pending::cancel_locked` call in `submit`'s supersede branch, `normalized_criteria`, and R1.5's `no_cache` line in `apply_check`.
- `PLAN-loom-ui-mods` (`assets/install*.rs`, `commands/install_assets.rs`): `prune_stale_agents` and `InstallReport.pruned_agents`.
- `PLAN-model-router-hooks` (`fs/permissions/constants.rs`): `_verify_matchers.sh` in `LOOM_HOOKS` and out of every hook table.
- `PLAN-web-host-graft-followthrough` (signal cache and its doctrine tests): BLOCK-A byte-identical across its four copies, and `cache.rs` at or under its 511-line baseline.
- Any knowledge stage edits R4's files only after R4 commits.

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
   - Integration-verify: report `ran` bare runs next to the count of `loom check integration-verify` calls. There is no zero target. Baseline: 30 subagent and 50 main hits.

## Pressure tests

- 2026-11-09: Claude opus, Codex gpt-6-astra; 1 round, review addressed
