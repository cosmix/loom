# review-harvest-gate / W2 — harvest hook, delegate, `loom stage review status`

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D12. Knowledge:
`architecture/hook-system.md` (embedding, registration, the hook output contract);
`mistakes/hooks-shell-portability.md`; `mistakes/untrusted-value-boundaries.md`;
`architecture/memory-spool.md`. Code: `loom-hooks/subagent-stop.sh` (154 lines) and
`loom-hooks/_lifecycle.sh` (the `loom_lifecycle_*` helpers);
`commands/subagents/classify/entry.rs` (`read_entries`, `text_blocks`, `is_assistant`, the same
transcript format the hook validates); `commands/hook/project_types.rs` (a hidden delegate reading
JSON on stdin).

Pinned from others:

- W1's store API: `verify::review::store::{next_round, write_round, load_rounds, open_findings, ReviewRound, RecordedFinding}`;
- the codex units' `verify::review::report::parse_review` and
  `verify::review::fingerprint::{compute, changed_since}`;
- W3's memory API: `MemoryEntryType::Suggestion`, recorded through the same function
  `loom memory note` uses (`commands/memory/handlers/record.rs::record_kind`, L91-108), called
  in-process.

## Files you own

`loom-hooks/subagent-stop.sh`, `loom-hooks/tests/subagent-stop-review-harvest.sh` (new),
`loom-hooks/tests/run-all.sh`, `loom/src/commands/hook/mod.rs`,
`loom/src/commands/hook/review_harvest.rs` (new), `loom/src/commands/hook/review_harvest_tests.rs`
(new), `loom/src/cli/types_ops.rs` (`HookCommands`), `loom/src/cli/dispatch.rs` (the hook arm;
`dispatch` is ledgered at 86, so extract to pay for lines), `loom/src/cli/types_stage.rs` (one
variant line), `loom/src/cli/types_stage_review.rs` (new), `loom/src/cli/dispatch_stage.rs`
(`dispatch_stage` 51 ledgered), `loom/src/commands/stage/mod.rs`,
`loom/src/commands/stage/review_status.rs` (new), `loom/src/sandbox/settings.rs` (`STATE_READ_DIRS`
only, edited in place).

## Tasks

1. `subagent-stop.sh`: after the lifecycle append and heartbeat refresh, when
   `AGENT_TYPE == loom-code-reviewer`, pipe
   `{"stage_id":…,"session_id":…,"agent_id":…,"transcript_path":…}` (built with `jq -n --arg`)
   to `loom hook review-harvest`, bounded by the existing timeout helper. Its failure never
   changes the hook's exit code or output. Log the failure only under `LOOM_HOOK_DEBUG=1`.
   Validate `AGENT_TYPE` against the exact string; no glob.
2. `commands/hook/review_harvest.rs` (hidden `HookCommands::ReviewHarvest`):
   - load the stage directly (hooks run outside the stage sandbox); do nothing unless
     `plan_version == 2`;
   - validate the transcript path the way the hook does: a plain file, no symlink, under the
     parent session's `subagents/` directory;
   - take the final assistant entry's text;
   - `parse_review`;
   - `compute` the fingerprint (target branch from `crate::fs::resolve_target_branch_from_config`,
     worktree from the stage);
   - assign ids;
   - write one Suggestion memory entry per suggestion (content `<file>:<line> <text>`, evidence
     `review round <n>`) and record their ids in the round;
   - `write_round`. On `Err(reason)` it writes the round with `malformed: reason` and no
     findings.

   Exit 0 in every case; print one line to stderr on malformed input.
3. `loom stage review status <stage-id>` (`types_stage_review.rs` holds a `ReviewCommands` enum
   with `Status`; `test-guards` adds `Integrity` there later). Output, for the main agent to paste
   into the next reviewer's brief:
   - the rounds (number, fingerprint, finding count, malformed reason);
   - open findings (id, severity, `file:line`, claim, scenario or rule);
   - whether the latest round matches the current fingerprint;
   - `changed since last round:` followed by the files from `changed_since`.
4. `sandbox/settings.rs` L39: add `"reviews"` to `STATE_READ_DIRS` (contract-phase already made
   the array longer; edit in place).
5. `loom-hooks/tests/subagent-stop-review-harvest.sh`, registered in `run-all.sh`, in the style of
   `subagent-stop-heartbeat-lock.sh`. It feeds the hook a `loom-code-reviewer` stop event with a
   fixture transcript (loom binary stubbed through `LOOM_BIN` or PATH as the existing tests do),
   asserts the delegate was invoked with the right JSON, and asserts a non-reviewer agent type
   does not invoke it.

## Named tests (binding), in `review_harvest_tests.rs`

- `harvest_writes_round_and_suggestions`: a temp work dir with a v2 stage and worktree, and a
  transcript whose final assistant message has a `loom-review` block with one finding and one
  suggestion → `round-1.json` with `F-1-1` and the fingerprint, and one pending
  `suggestion` memory entry in the stage journal.
- `harvest_skips_v1_stage`: the same with `plan_version == 1` → no files written.

## Proof (one command, once, after W1, W3 and the codex units return)

`cargo test --manifest-path loom/Cargo.toml --lib commands::hook::review_harvest`

## Report

Files changed; exact counts of `dispatch`, `dispatch_stage`, `sandbox/settings.rs`; the hook
test result (`bash loom-hooks/tests/subagent-stop-review-harvest.sh`); the proof result.
