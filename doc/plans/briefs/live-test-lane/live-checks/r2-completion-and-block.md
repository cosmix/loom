# live-checks R2: the completion check, the block rule, the profile-test note (pass and fail output), status

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first ("Receipts and
completion"). Frozen contracts: `tests/live_checks_contracts.rs` (with
`tests/live_checks_contracts/**`); never edit them. The stage description's PUBLIC SURFACE is
authoritative. R1 writes the receipt types (`crate::live::receipt`) in parallel; code against
the PUBLIC SURFACE names.

## Files (yours alone)

`src/live/completion.rs` (the foundation step created it with one `//!` line),
`src/live/tests_completion.rs` (declared from `completion.rs` as `#[cfg(test)] #[path =
"tests_completion.rs"] mod tests;`), `src/commands/stage/complete_verification.rs`,
`src/stage_service/control_block.rs`, `src/stage_service/control_block_agent_tests.rs`,
`src/commands/stage/criterion_output.rs`, `src/cli/types_live.rs`,
`src/commands/live/mod.rs`, `src/commands/live/status.rs`, `src/orchestrator/signals/merge.rs`,
`src/orchestrator/signals/merge_live.rs` (new), `src/orchestrator/signals/merge_live_tests.rs`
(new), `src/orchestrator/signals/mod.rs`. Also frozen for you:
`src/orchestrator/signals/live_signal_contract_tests.rs` and
`src/commands/stage/criterion_output_contract_tests.rs` (declared from
`src/commands/stage/mod.rs`, a file you never edit: it holds the derived scaffold line).

## `src/live/completion.rs`

- `evaluate(stage_id, entries, receipt, current) -> LiveVerdict` (pure): the receipt counts
  only when `receipt.binding` is `Some` and equals `current`; within it, an entry's outcome is
  the receipt entry whose `definition_hash` equals `live_entry_hash(entry)` (matched by hash,
  never by position, so amending one entry leaves the others' outcomes valid). A required entry
  without a counting receipt or without a matching outcome is one failure `no live run matches
  this content and these live entries: run loom check --live <stage>` (once, however many
  entries lack one); an advisory entry without one gets `live #<n> has no current run`.
  Otherwise, per entry `n` (1-based in text, 0-based in `[live <i>]` gap labels, as
  `--criterion-index` takes them):
  - required and `Passed`: nothing; required and `Failed`: `[live <i>] failed in the live run
    at <time>: <reason>; fix it and run loom check --live <stage>, or dispute it with loom stage
    dispute-criteria <stage> --field live --criterion-index <i>`;
  - required and `Unavailable { Credential }`: `[live <i>] could not run at <time>: <reason>; a
    person provides the credential, so this stage may loom stage block with that reason`;
  - required and `Unavailable { Facility }`: `[live <i>] could not run at <time>: <reason>;
    dispute it with loom stage dispute-criteria <stage> --field live --criterion-index <i>`;
  - not required: one advisory line `live #<n> (advisory) <outcome> at <time>`.
- `live_completion_check(stage, checkout, work_dir) -> Result<()>`: load the receipt (an
  unreadable one counts as none, with a `tracing::warn!`), compute `current_binding(checkout)`,
  `evaluate`, print every advisory, and `bail!` with the failures joined by `; ` when there are
  any. It runs no live entry and starts no process but git.
- `block_refusal(stage_id, failing_labels, entries, receipt, current) -> Option<String>`
  (pure): `None` unless `failing_labels` contains `live`; `None` when the receipt counts and
  every required entry that is not `Passed` is `Unavailable { Credential }`; otherwise
  `cannot block stage '<id>': its live check failed for a reason a person is not needed for:`
  followed by the `evaluate` failures, one per line.
- `pub fn live_profile_note(stdout: &str, stderr: &str) -> Option<String>` (pure; `pub`, not
  `pub(crate)`, because the contract `criterion-output-counts-profile-tests` in
  `tests/live_checks_contracts.rs` calls `loom::live::completion::live_profile_note` from
  outside the crate, and the plan's PUBLIC SURFACE declares it there): count distinct test
  names from libtest lines `test <name> ... ignored, loom-live-profile:...` and guard lines
  `loom-live-profile: skip <name>:`; `Some("note: <n> tests need the live profile and did not
  run here (loom check --live <stage> runs a stage's live entries)")` when `n > 0`. Its unit
  tests go in `tests_completion.rs`.

## Wiring

- `complete_verification.rs` `run_checks`: after the `after-stage` check, `if
  checks.stage.plan_version == 2 && !checks.stage.live.is_empty() { sink("live",
  crate::live::completion::live_completion_check(checks.stage, checkout, checks.work_dir))?; }`
  with `checkout` = `checks.acceptance_dir` (else `Path::new(".")`). Keep `run_checks` under 50
  lines.
- `control_block.rs`: split `failing_criterion_indices` into `latest_failing_labels(work_dir,
  stage_id) -> Vec<String>` (every failing label of the current session's latest trusted
  `ToolFailed` attempt; unreadable records yield none, as today) and the acceptance indices
  derived from it. `handle_agent_block_stage` keeps the acceptance refusal first, then, when the
  labels hold `live`, loads the stage, its receipt and `current_binding(<stage worktree>)`
  (`crate::git::worktree::stage_worktree_path`) and returns `Response::Error { message }` for
  `block_refusal`'s `Some`. Rewrite the doc comments to the current rule. The
  autonomous-recovery plan, which runs after this one, replaces this guard with its own
  dispute-route table and must keep this rule; keep the live decision in
  `crate::live::completion::block_refusal` so that rewrite calls one function.
- `control_block_agent_tests.rs`: one test where the latest attempt failed only `live` and the
  receipt says `Unavailable { Credential }` (blocked), one where it says `Failed` (refused with
  `--field live`). Mirror the file's signed-checkpoint fixture.
- `criterion_output.rs` (165 lines; `format_criterion_result` is lines 21-61, 41 lines): the
  note exists for PASSING runs. A test that needs the live profile is ignored (`test <name> ...
  ignored, loom-live-profile: ...`) or skipped by the guard (`loom-live-profile: skip <name>:`),
  and the criterion still passes, so the note is the only sign those tests did not run. The
  function is defined in `completion.rs` (above), never in this file; the plan's wiring check
  looks for the call `live_profile_note(` here. Add this helper (it is the only call site):

  ```rust
  /// Push the live-profile note, indented like a failure's detail lines, when the
  /// result's captured output shows tests that need the live profile.
  fn append_live_profile_note(lines: &mut Vec<String>, result: &CriterionResult) {
      if let Some(note) = crate::live::completion::live_profile_note(&result.stdout, &result.stderr)
      {
          lines.push(format!("    {note}"));
      }
  }
  ```

  Then, in `format_criterion_result`:
  1. Passing, cached: the first early return (`  ✓ passed (cached): <cmd>`) stays as it is and
     carries no note. A cached `CriterionResult` (`cached_verdict`, `verify/criteria/result.rs:53-73`,
     built from the cache record at `verify/criteria/runner.rs:200-208`) holds only a 4 KiB tail
     of each stream (`DIAGNOSTIC_TAIL_BYTES`, `verify/criteria/cache_contract.rs:16`; the tails are cut
     at `cache_contract.rs:223-224` and `:259-268`), and `result.rs:22-25` says a hit is too short
     to re-count from. A count over a tail undercounts, and a wrong number is worse than none.
     Giving the cache record a count needs a field in `cache_contract.rs` and a record version
     bump, files this stage does not own. Revisit when a cache record stores the profile-test
     count (as it stores `tests_executed`): the cached branch then prints the note too.
  2. Passing, not cached: build `let mut lines = vec![format!("  ✓ passed: {}", result.command)];`,
     call `append_live_profile_note(&mut lines, result);` and return `lines.join("\n")`. When
     `live_profile_note` returns `None` the output is the single pass line, byte for byte what it
     is today.
  3. Failing or timed out: call `append_live_profile_note(&mut lines, result);` once, after the
     stderr tail and then the stdout tail are pushed (that order is unchanged) and before
     `lines.join("\n")`, so the note is the last line. The note counts the whole captured output,
     which the 20-line tails may not show.
  4. Size: `format_criterion_result` ends near 45 lines (limit 50) and the file near 230 (limit
     400), tests included; if either grows past that, move code into helpers in this file.
  5. Replace the doc comment's last paragraph (lines 16-20, from "A passing result renders
     exactly as before") with:

     ```rust
     /// A passing result renders as one line, plus one indented `note:` line when its
     /// captured output shows tests that need the live profile and did not run
     /// (`live_profile_note`). A cached pass renders one line always: the cache keeps
     /// only a short output tail, too short to count from. A FAILED or TIMEOUT result
     /// additionally shows the exit code, the setup commands (when any ran), the tails
     /// of stderr and stdout, and last the same note — otherwise the cause of a failure
     /// stayed invisible until someone re-ran the command by hand.
     ```

  6. The four existing tests in this file's `tests` module (lines 109-164) stay unchanged and
     must still pass: their fixtures (`result` builds an empty stdout; stderr is a mkdir error,
     `warning: something noisy`, `line N`, `still running when killed`) hold no profile lines.
     Do not change `result`'s signature. Add three tests beside them, building results with
     `CriterionResult::new` and a stdout of `test a ... ignored, loom-live-profile: drives Claude
     Code`: a passing result gives two lines (the pass line, then four spaces and the note); a
     passing result with `cached` set gives the cached line alone; a failing result ends with the
     note line, after the `stdout:` tail.
  7. `Progress::acceptance` (`host_verify/child.rs:354`, R1's file) calls this function and stores
     the string as the criterion's summary, so the relayed `loom check` output carries the note
     with no edit there.
- `types_live.rs` `Status { #[arg(long, value_parser = clap_id_validator)] stage:
  Option<String> }`; `commands/live/mod.rs` routes it; `status.rs` with `--stage` prints, after
  the four lines, one line per entry: `live #<n> <required|advisory> <outcome> at <time>` from a
  counting receipt, or `live: no current run (run loom check --live <stage>)`.

## The merge resolver's live section (plan Decision 16)

`verify_resolved_completion` (`commands/stage/complete_resolved.rs`) is what `--resolved`
(`inbox_drain/merge_resolved.rs`) and a resolver's exit (`merge_handler/resolver_exit.rs`)
run. It calls `complete_verification::run`, so your `live` check runs there too, with
`checkout` = the acceptance dir inside `.worktrees/<stage>`, the worktree the resolver merges
in (`merge_resolver_worktree`). `content_digest` hashes that whole worktree (`checkout_git`
resolves the enclosing stage worktree), as it does for the live job's own checkout, so a
receipt the resolver records there binds the tree completion checks. The merge changed the
content, so the receipt from before it no longer counts. Edit none of those three files.

- `src/orchestrator/signals/merge_live.rs` (declare `mod merge_live;` in `signals/mod.rs`
  beside `mod merge;`): `pub(super) fn format_live_section(stage: &Stage) -> String`, empty
  unless `stage.live.iter().any(|entry| entry.required)`. Otherwise a `## Live Entries` section
  that says, in this order:
  1. This stage has required `live:` entries; completion accepts them only from a live run of
     the content being landed, and the merge changed that content.
  2. After step 4 (the merge committed, the worktree clean) and before step 5
     (`loom stage merge <stage> --resolved`), run `loom check --live <stage>`; when it returns
     before the run ends, run it again until it prints each entry's outcome.
  3. Any later commit changes the content again: rerun `loom check --live <stage>` before the
     next `--resolved`. A `--resolved` refusal that names `loom check --live` means the same.
  4. `[live <i>] failed`: fix what the merge broke, commit with `loom stage commit`, then item 2
     again. A failure the merge did not cause, or one that persists: `loom stage block <stage>
     "<the [live <i>] line>"`.
  5. `[live <i>] could not run` (credential or facility): a resolver can neither dispute nor
     set a credential, so `loom stage block <stage> "<the [live <i>] line>"` at once; a person
     provides what is missing. Ignore the line's dispute hint: a resolver may not dispute.
  Never write "Do NOT re-run" or "already passed" (tests in `tests_merge.rs` forbid both).
- `src/orchestrator/signals/merge.rs` is 396 lines: add exactly one line to
  `format_merge_signal_content`, `content.push_str(&super::merge_live::format_live_section(stage));`
  right after the `format_merge_task` push, so it ends at 397. The autonomous-recovery plan's
  stall-recovery W2 later rewrites step 5's wording in this file and keeps that call.
- `src/orchestrator/signals/merge_live_tests.rs`, declared from `merge_live.rs` as
  `#[cfg(test)] #[path = "merge_live_tests.rs"] mod tests;`: the section is empty for an
  advisory-only stage and for one with no entries; a required entry gives the section naming
  the stage id in both commands.

What each path does once the section is in place (the contracts named in the plan prove the
starred rows):

| Path | Receipt at `--resolved` | What loom does | Resolver's move | End |
| --- | --- | --- | --- | --- |
| Every required entry passes (*) | bound to the resolved tree, `passed` | `live` passes in `verify_resolved_completion` | step 5 | merge lands |
| A required entry fails | resolved tree, `failed` | `--resolved` refused with the `[live <i>] failed` line | fix what the merge broke, commit, rerun; otherwise give up with `loom stage block` | lands after the fix, else human review with the line |
| Unavailable: facility | resolved tree, `unavailable (facility)` | refused with the `[live <i>] could not run` line | `loom stage block` at once | human review; a person fixes the host or amends the entry |
| Unavailable: credential | every entry `unavailable (credential)`, nothing ran | refused, the line names `loom live credential set` | `loom stage block` at once | human review; the operator sets the credential |
| Resolver skips the run (*) | the pre-merge receipt, which no longer counts | `--resolved` refused: `run loom check --live <stage>`; a resolver that ends with the merge committed but without `--resolved` reaches `land_resolved_worktree`, which routes the stage to human review with that text | run it, then step 5 | lands, else human review |
| Entry hits its `timeout_secs` | resolved tree, `failed` (timed out) | as a failed entry | rerun once, then as a failed entry | as a failed entry |
| Call returns before the job ends (100 s at most) | not written yet | `Pending`; the job keeps running | rerun `loom check --live <stage>`: it follows the same job | as the job's outcome |
| Job killed at its deadline | none written | as a skipped run | rerun | as a skipped run |
| Tree changed after the run (*) | binding differs from the current content | refused: `run loom check --live <stage>` | rerun after the last change, then step 5 | lands |
| Tree changed during the run | `binding: None` | as above | as above | lands |

Corner cases: a stage with only advisory entries gets no section and needs no run (the `live`
check prints `live #<n> has no current run` and passes); a stage with no entries gets no section
and no `live` check at all; a resolver that resolves twice (the target moved and `--resolved`
answered with a new conflict) commits a second merge, which voids the first run, so item 3 sends
it back to item 2. A later resolver attempt gets a fresh signal with the same section; a receipt
left by the previous attempt counts only when its binding equals the current content.

## Traps

- Completion never runs a live entry: `live_completion_check` takes no runner and spawns
  nothing.
- `control_block.rs` is 288 lines; keep the new reads in helpers. `signals/merge.rs` 396:
  one line only. `criterion_output.rs` is 165 lines and `format_criterion_result` 41: stay
  under 400 and 50.
- `cargo fmt`; your one check: `cargo test --lib live::completion --no-run`.

## Report

Files changed, the final length of `run_checks`, the final `wc -l` of `signals/merge.rs`, the
final `wc -l` of `criterion_output.rs` and the final length of `format_criterion_result`, and
the exact refusal text a credential-free failing `live` produces.
