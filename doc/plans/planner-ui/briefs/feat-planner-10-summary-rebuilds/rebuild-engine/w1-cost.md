# The cost library (reference for codex units C1 and C2)

This file is the rule reference for codex units C1 (`c1-cost.md`: cost.rs) and C2
(`c2-estimate.md`: estimate.rs); it is no longer a worker brief. Read
`doc/plans/briefs/feat-planner-10-summary-rebuilds/common.md` ("Cost model") first.

## Files you own

`loom/src/planning/summary/cost.rs`, `loom/src/planning/summary/cost/estimate.rs`,
`loom/src/planning/summary/cost/tests.rs`, `loom/src/planning/summary/mod.rs` (the seed added
`pub mod cost;`; add nothing else). The frozen contract file
`loom/src/planning/summary/cost/contract_tests.rs` is declared by the derived scaffold
(`#[cfg(test)] mod contract_tests;` in `cost.rs`): keep that line.

## API

The module: `loom::planning::summary::cost` (no server dependency):
- `pub struct TokenVector { pub fresh_input: Option<u64>, pub cache_write: Option<u64>, pub cache_read: Option<u64>, pub output: Option<u64> }` (absent, never zero, when unknown).
- `pub enum Trigger { Refinement, Background, Button }` (serde snake_case).
- `pub struct CostLine { pub time: String /* RFC 3339 */, pub trigger: Trigger, pub summaries: Vec<String>, pub level: Level /* document|overview */, pub model: Option<String>, pub source_bytes: Option<u64>, pub tokens: TokenVector }`;
  `pub fn append(paths: &RuntimePaths, line: &CostLine) -> Result<()>` (locked append to `summary-cost.jsonl`), `pub fn read_log(paths) -> Result<Vec<CostLine>>` (bad lines skipped).
- `pub fn transcript_tokens(path: &Path) -> Result<TokenVector>`: complete lines through `tokens::accumulate::AgentState::{new, ingest}` (the tracker's parser and message-id merge, so counts match `loom status`).
- `pub fn estimate(log: &[CostLine], pending: &[(String /* id */, Level, u64 /* source bytes */)]) -> Estimate { tokens: u64, per_summary: Vec<(String, u64)>, basis: EstimateBasis /* last|per_byte|starting */ }`.
- `pub fn totals(log, now) -> Totals { today: TokenVector, week: TokenVector }`.


- `TokenVector`, `Trigger`, `Level`, `CostLine` with serde (`snake_case`, `skip_serializing_if` on
  every `Option`).
- `transcript_tokens(path) -> Result<TokenVector>`: open read-only (`std::fs::File`; transcripts are
  Claude Code's own files under `~/.claude/projects`, not session-writable through the sandbox's
  allow list), read complete lines only (a final line without `\n` is skipped: Claude Code may be
  writing it), feed each to `AgentState::ingest` (`tokens/accumulate.rs:117`), map `total`. Lines
  longer than `tokens::tail::MAX_LINE_BYTES` (32 MiB, `tokens/tail.rs:12`, `pub(super)`: copy the
  bound as a local const with a comment naming its origin) are skipped.
- `append(paths, &CostLine)`: open `paths.summary_cost` with `O_APPEND | O_CREAT | O_NOFOLLOW`, mode
  0600, `fs2::FileExt::lock_exclusive`, write one line, unlock.
- `read_log(paths)`: bounded read (8 MiB) through `fs::safe_read::read_bounded`; unparsable lines
  skipped.
- `estimate.rs`: `estimate(log, pending)` per common.md and plan Decision 8 (median per level of
  `fresh tokens / source bytes` over log lines whose single summary has a recorded source size;
  record the source size on each `CostLine` as `source_bytes: Option<u64>`, add that field);
  `totals(log, now) -> Totals { today, week }` (local-time day boundary with `chrono::Local`, week =
  last 7 x 24 h).

## Tests (yours, `cost/tests.rs`)

Absent fields omitted in JSON; append from two threads produces two whole lines; a partial final
line skipped; `totals` across a day boundary; estimate basis per pending summary (`last`,
`per_byte`, `starting`).

## Proof

`cargo test --lib planning::summary::cost::`, `cargo clippy --all-targets -- -D warnings`.
