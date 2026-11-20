# K2 — memory note shape, pending grouping, distill procedure

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first, in particular the "Memory
grouping" contract.

## Goal

Distillation starts from a grouped worklist instead of searching for where each memory belongs,
and notes arrive in a shape that can be distilled. Evidence: report section 4.8 — 9,953 output
tokens and 2.1 greps per knowledge write, 235k mean peak context; 10% of 1,336 notes carry the
mistake-plus-prevention shape; the prefixes `mistake:`, `stale-knowledge:`, `found/gotcha:` are
prompt conventions no code parses.

## Files you own (write)

- `loom/src/commands/memory/handlers/record.rs`, `handlers/pending.rs`, a new
  `handlers/prefix.rs`, and their tests
- the memory dispatch code that maps `MemoryCommands::Pending` to `pending()` (find it with
  `loom map --find-all pending`). `loom/src/cli/types_memory.rs` belongs to K1, who adds
  `group: bool` to the `Pending` subcommand; you read that field and do not edit the file
- `commands/distill.md` (repo root, the `/distill` command)
- `loom/src/orchestrator/signals/cache.rs` — `generate_knowledge_distill_stable_prefix` only
  (166-230), and the tests that assert its text

## Where things are

- `note()` — `record.rs:189-191`, delegates to `record_kind`; the spool fallback for worktree
  sessions is at 160-186 (`doc/loom/knowledge/architecture/memory-spool.md`).
- `pending()` — `pending.rs:30`, `pending_report` at 100. Resolve outcomes: `promoted`, `merged`,
  `discarded`, `deferred`.
- Prefix conventions as the template states them: `mistake: tried X because ... Failed because Y.
  Prevention: ... Fix: Z`; `stale-knowledge: <knowledge file>#<heading> claims X; the tree does Y
  (file:line). Correction: <replacement text>`; `found/gotcha: ... in file:line`.
- The distill stage's procedure is emitted by `cache.rs` as literal text: memory ordering
  doctrine at 181, a seven-step workflow, the corrections pass at step 6 (219). `/distill` is a
  separate six-step document. Neither is generated from the other.

## Steps

1. `prefix.rs`: `enum NotePrefix { Mistake, StaleKnowledge { file, heading }, Found, None }` and
   a parser over a note's text. `StaleKnowledge` parses `<file>#<heading>` up to the first
   ` claims ` or `;`.
2. Shape checks in `record_kind`, before anything is written or spooled, so both paths agree. A
   note starting `mistake:` must contain `Prevention:`. A note starting `stale-knowledge:` must
   parse to a file and heading and contain `Correction:`. On failure: exit non-zero, print the
   expected shape in two lines, write nothing. Every other note is accepted unchanged.
3. `loom memory pending --group` per the contract: four groups; within `corrections`, sort by
   target file then heading, and print the target in a column of its own so a distiller can go
   file by file. `--json --group` carries the same structure. `--strict` keeps its meaning.
4. Distill procedure, in both texts: the first step becomes "run `loom memory pending --group`
   and work the groups in order: corrections (apply each with `replace-section` against the
   printed target), then mistakes, decisions, other". Add one instruction: when a mistake is a
   recurrence of one already in the tree, record a proposal for a hook or a `loom plan verify`
   check in `concerns.md` instead of another paragraph, and resolve the memory as `merged`.
   Keep every existing step; renumber.

## Traps

- `doc/loom/knowledge/mistakes/memory-relay-drain-gap.md` and `architecture/memory-spool.md`: a
  note can arrive through the spool later and out of order; validation happens at the CLI entry
  only, never at drain time, or a valid note written by an older binary is lost.
- Tests asserting signal text live beside `cache.rs` and in `tests_doctrine*.rs`. The doctrine
  stage edits other functions in `cache.rs` after this stage merges; keep your change inside the
  one function.
- The stale-knowledge heading may itself contain `#` or `:`; split on the first `#` only.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib commands::memory` and
`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib orchestrator::signals::cache`
— each run once.
