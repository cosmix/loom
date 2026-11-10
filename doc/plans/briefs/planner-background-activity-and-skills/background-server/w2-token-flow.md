# W2: the planner token pass (step A2)

Read `common.md` in this directory first. You are a `loom-software-engineer`. You run in wave 1 beside W1 (`src/agent_state/**` and the subagent classifier), which shares the crate: a build failure in W1's files is W1's half-written code; check again later.

## Goal

The plan server gets per-agent token figures for the chat session from the same reader, flow builder and checkpoint the daemon uses for stages, without touching the daemon's output. Decision A-D1c in the plan is the specification; this brief settles where each piece goes.

## Files owned

`src/tokens/**`. Read only: everything outside `src/tokens/`, in particular `tests/token_flow_contracts.rs` and `tests/token_flow_contracts/`, which must pass unchanged, and the frozen `src/tokens/planner_contract_tests.rs` (the contract `a-restarted-planner-pass-does-not-double-count`; keep the `#[cfg(test)] mod planner_contract_tests;` line the contract session added to `tokens/mod.rs`).

## What exists (verified 2026-11-10)

- `tokens/mod.rs:21` exports only `scan_pass`, `spawn_token_tracker`, `CHECKPOINT_FILE`, `TRACKER_INTERVAL_SECS`; everything else is private or `pub(super)`.
- `tracker.rs:61-72` `scan_pass` runs `scan_until` with the thread-local `TRACKER` (`:36-50`) memo; `:98-111` `scan_until` keys the memo by work root (`memo.is_for(work_root)`); `:114-135` `run_pass`: `ensure_tokens_dir(work_root)`, `memo.take_checkpoint(&dir)`, `discover(work_root, …)`, `read_sources`, `checkpoint.prune`, `build_flow`, `memo.quota_left`, `memo.save_checkpoint`, `memo.publish_flow`, `writes_outcome`. `:139-200` `read_sources` and `read_source`. `:222-232` `ensure_tokens_dir` refuses a symlink and creates the directory 0700. `:244-281` `run_tracker`, `guarded_pass` (catches a panic), `log_change`, `sleep_unless_shutdown`.
- `pass_memo.rs:65-115` `PassMemo { work_root, … }`, `new(work_root)`, `is_for(work_root)`; `take_checkpoint(tokens_dir)` and `save_checkpoint(…, tokens_dir, advanced)` already take the directory.
- `checkpoint.rs:14-15` `CHECKPOINT_VERSION = 4`, `MAX_CHECKPOINT_BYTES` 8 MB; `:28-34` `Checkpoint { version, stages: BTreeMap<String, StageAgents>, threads }`; nothing ever removes a stage.
- `discover.rs:127-165` `session_sources` builds a `main-<stem>` source and one subagent source per `agent-<id>.jsonl`, skipping stems and ids that fail `is_safe_id`, and takes `agent_type` from the stage ledgers.
- `flow_build.rs:21-49` `build_flow(&stages, now)` builds one `StageFlow` per checkpoint stage; `:113-153` a subagent's published id is its key, the full agent id.
- `planning/runtime.rs:30-45` `RuntimePaths.dir` is the plan runtime directory; the plan server passes `<RuntimePaths.dir>/tokens` as `tokens_dir` (W4).

## Steps

1. **Memo key.** Rename `PassMemo.work_root` to `root` (doc: the directory the memo belongs to: a work root for the daemon's pass, a tokens directory for the planner's). `scan_until` keeps keying by work root; the planner pass keys by its tokens directory. Each runs on its own thread, so the two never share a memo.
2. **Directory check.** Split `ensure_tokens_dir(work_root)` into `ensure_dir(dir: &Path) -> Result<PathBuf>` (the symlink refusal, 0700 create and directory check, unchanged) and the daemon's `ensure_tokens_dir(work_root) = ensure_dir(&work_root.join(TOKENS_DIR))`.
3. **Sources.** In `discover.rs`, factor the two `Source` constructions of `session_sources` into `main_source(stage_id, transcript) -> Option<Source>` and `subagent_source(stage_id, agent_id, path, agent_type) -> Option<Source>` with today's `is_safe_id` rules, and call them from `session_sources`. Keep `discover.rs` at most 400 lines (299 today). The private `Source.stage_id` keeps its name.
4. **`Checkpoint`.** Add `pub(super) fn retain_stages(&mut self, keep: impl Fn(&str) -> bool)`. The file format and `CHECKPOINT_VERSION` do not change.
5. **`src/tokens/planner.rs`** (new; `mod planner;` and `pub use planner::{planner_pass, spawn_planner_tokens, PlannerGroup};` in `tokens/mod.rs`). Signatures in `common.md`. `planner_pass`:
   - `ensure_dir(tokens_dir)`; take the thread's `TRACKER` memo when `is_for(tokens_dir)`, else a fresh `PassMemo::new(tokens_dir)`, and put it back afterwards, as `scan_until` does;
   - build the sources: for each group, `main_source(&group.id, main)` and one `subagent_source` per entry, with the group's `agent_type`;
   - `memo.take_checkpoint(dir)`; `checkpoint.retain_stages(|id| groups name it)`, so a group the caller no longer passes leaves the checkpoint; `read_sources` with the thread's stop flag; `prune` as `run_pass` does;
   - `build_flow(&checkpoint.stages, now)` and return `(stage_id, agents)` for each `StageFlow`, in group order;
   - `memo.save_checkpoint(checkpoint, dir, &advanced)` and `writes_outcome`-style error handling for the save alone; no `flow.json`, no `quota_left`.
6. **`spawn_planner_tokens`** in `planner.rs`: the thread loop of `common.md`, reusing `guarded_pass`'s panic catch, `log_change` and `sleep_unless_shutdown` (make them `pub(super)` in `tracker.rs`; do not copy them), with `TRACKER.stop` set to the thread's stop flag as `run_tracker` sets it, so a pass stops reading files at shutdown. `thread::Builder::new().name("planner-tokens")`, returning the spawn error instead of panicking.
7. **`run_pass` and `scan_pass` keep today's behaviour exactly**: `discover(work_root)`, `flow.json`, `quota_left`, the memo and the warnings. Share code with the planner pass only through the pieces above.
8. **Tests** in `src/tokens/planner_tests.rs` (declared from `planner.rs` with `#[cfg(test)] #[path = "planner_tests.rs"] mod tests;`, as the other `tokens` modules do), over copies of `tests/fixtures/plan_session/transcripts/subagent-main.jsonl` and `subagent-agent.jsonl` (agent `a7f3`) in a TempDir:
   - (a) one group yields two rows: `main-<stem>` with its model and a non-zero total, and `a7f3` with `agent_type` `Explore`, its model and a non-zero total;
   - (b) a second pass over unchanged files returns the same totals and leaves `checkpoint.json` byte-identical;
   - (c) a pass without the group removes it from `checkpoint.json`;
   - (c2) planner passes in one tokens directory never create or change `<work_root>/tokens/checkpoint.json` of a fixture work root next to it (`tests_fixture.rs` builds one); the fresh-memo restart case is the frozen contract's;
   - (d) the existing `tokens::` tests and `tests/token_flow_contracts.rs` pass unchanged.
9. **Run** `cargo test --lib tokens::` and `cargo test --test token_flow_contracts` once your code is ready.

## Done when

`planner_pass` and `spawn_planner_tokens` exist as specified, the daemon pass is unchanged, the frozen contract and the tests above pass, and every `tokens/` file is at most 400 lines with functions at most 50.

## Traps

- The teammate filter and the meta-file `agentType` are W4's: `planner_pass` reads exactly the subagents it is given.
- `build_flow` publishes main and codex ids in short form (`flow_build.rs:144-152`); only subagent ids are the join key, so leave `short_id` as it is.
- A pass must not hold a lock or call back into the plan server; the sink is called after the pass returns.
