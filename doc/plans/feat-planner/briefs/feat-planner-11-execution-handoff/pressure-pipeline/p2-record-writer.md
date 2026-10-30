# P2: the `## Pressure tests` record writer

LANE: this brief is now the SPECIFICATION for codex units R1 (`record.rs`) and R2 (`record/tests.rs`) in `doc/plans/briefs/feat-planner-11-execution-handoff/pressure-pipeline/codex-units.md`. No Claude worker is spawned from it.

Stage `pressure-pipeline` of `doc/plans/PLAN-feat-planner-11-execution-handoff.md`. Tier: sonnet
(`loom-software-engineer`). Never run git, never spawn subagents. One narrow check at the end:
`cargo test --lib pressure::record::` from `loom/` (it compiles once P1 has declared
`pub mod record;` in `src/pressure/mod.rs`; if it has not yet, report and stop). The frozen
contract file `tests/pressure_pipeline_contracts.rs` (and `tests/pressure_pipeline_contracts/**`)
is READ-ONLY.

## Files you own (relative to `loom/`)

- `src/pressure/record.rs` (ends with `#[cfg(test)] mod tests;`)
- `src/pressure/record/tests.rs`

## Spec

`doc/product/section_11.md` lines 150-198 ("The pressure record") and `doc/product/section_01.md`
lines 103-134 ("Pressure tests"). Plan parsing reads only the metadata block
(`src/plan/parser/extraction.rs:75`, markers `<!-- loom METADATA` and `<!-- END loom METADATA`).

## API (P1 calls exactly this)

```rust
pub struct RecordLine {
    pub date: chrono::NaiveDate,
    pub claude_model: String,
    pub codex_models: Vec<String>,   // one per round, in round order
    pub rounds: u32,
    pub missing_rounds: Vec<u32>,    // 1-based rounds whose report was absent at /address
}
impl RecordLine { pub fn render(&self) -> String }
pub enum RecordWrite { Written, PlanMoved }
pub fn append_record(plan_path: &Path, line: &RecordLine) -> anyhow::Result<RecordWrite>
pub fn append_to_text(text: &str, rendered: &str) -> String   // pure placement, for tests
```

## `render`

`- <YYYY-MM-DD>: Claude <claude_model>, Codex <codex>; <n> round|rounds, <outcome>`

- `<codex>`: when every entry of `codex_models` is equal, that one model; otherwise every
  round's model in round order joined with ` / ` (rounds `a, a, b` give `a / a / b`). An empty
  entry (no Codex model reported for a round) renders as `unknown`.
- `1 round`, `2 rounds`.
- `<outcome>`: `review addressed` when `missing_rounds` is empty; otherwise
  `Codex review missing in round <k>` with the rounds joined by `, ` (`Codex review missing in
  round 1, 3`).

Example: `- 2026-10-26: Claude opus, Codex gpt-6.1-sol; 2 rounds, review addressed`.

## `append_to_text` placement (section 11 "Where")

1. Find the last line starting with `<!-- END loom METADATA`. Only text AFTER it is searched.
2. If a line `## Pressure tests` (exactly, trailing whitespace ignored) follows the marker,
   its section runs to the next line starting with `#` (any heading level up to 2, i.e.
   `# ` or `## `) or the end of file. Insert the line directly after the last non-blank line of
   that section (its list items included), preserving every other byte.
3. Otherwise (no heading after the marker, or no marker at all) append at the end: ensure the
   text ends with `\n`, then `\n## Pressure tests\n\n<line>\n`.
4. A `## Pressure tests` heading BEFORE the marker (in plan prose) is never used.
5. The result always ends with exactly one `\n` after the inserted line when it was the last
   line. CRLF files: detect `\r\n` in the input and write the inserted lines with `\r\n`.

## `append_record` (section 11: read, append in memory, temp file renamed over, mode kept)

- `std::fs::symlink_metadata(plan_path)`: missing, or not a regular file (a symlink included),
  returns `Ok(RecordWrite::PlanMoved)` and writes nothing.
- Read the file (bounded: refuse over 8 MiB with an error), `append_to_text`.
- Write a temp file in the SAME directory, named `.<file name>.loom-record-<pid>-<nanos>`,
  opened `OpenOptions::new().write(true).create_new(true).custom_flags(libc::O_NOFOLLOW)` with
  `.mode(original_mode & 0o7777)` (`std::os::unix::fs::{OpenOptionsExt, PermissionsExt}`), then
  `set_permissions` on the open handle to the original mode (umask), `write_all`, `sync_all`,
  `rename` over `plan_path`, then fsync the directory (as `src/fs/locking.rs:75-110`
  `atomic_write` does). On any error remove the temp file and return the error.
- Before renaming, `symlink_metadata(plan_path)` again: if the file is gone or no longer the same
  inode (`MetadataExt::ino`), remove the temp file and return `PlanMoved`.

## Tests (`src/pressure/record/tests.rs`, yours; the contracts cover the core cases too)

Placement rules 1-5 each; CRLF; a section followed by `## Notes`; a heading inside the
metadata's YAML text (`## Pressure tests` inside a `description: |` block before the end marker)
ignored; render for 1 round, mixed models, `unknown`, several missing rounds; mode 0640 kept;
symlinked plan path → `PlanMoved` and target unchanged; missing plan → `PlanMoved` and nothing
created.

Keep `record.rs` under 400 lines and every function under 50.
