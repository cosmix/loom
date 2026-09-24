# review-harvest-gate / W3 — `suggestion` memory kind, `implemented` receipt

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D12 (memory bullet), D16 (IV and
knowledge-distill bullets). Knowledge: `architecture/memory-spool.md` "Entry Identity, Evidence
and Receipts" and "Pending Events and the Run Archive".

## Files you own

`loom/src/fs/memory/types.rs`, `loom/src/fs/memory/query.rs` (`generate_summary` ledgered 52),
`loom/src/fs/memory/export.rs` (`format_memory_for_signal` 92 and `format_memory_for_handoff` 80,
both ledgered), `loom/src/fs/memory/export_suggestions.rs` (new),
`loom/src/commands/memory/handlers/pending.rs` (382), `loom/src/commands/memory/handlers/pending_groups.rs`
(new, if `pending.rs` would pass 400), `loom/src/commands/memory/handlers/resolve.rs`,
`loom/src/commands/memory/handlers/suggestion_tests.rs` (new).

## Tasks

1. `types.rs`:
   - `MemoryEntryType::Suggestion`: `display_name` "Suggestion", an emoji, `all()`, `Display`
     "suggestion", `FromStr` "suggestion" or "suggestions" (FromStr has a wildcard, so the
     compiler will not remind you);
   - `ReceiptOutcome::Implemented`: `Display` "implemented", `FromStr`.
2. `pending.rs`: suggestions are pending until they have a receipt (L258-261), and
   `GroupedPendingReport` gains a `suggestions` bucket (L150-195). The JSON output includes it.
   Move grouping into `pending_groups.rs` if the file would pass 400 lines.
3. `resolve.rs`: `implemented` requires `--reason` (like `discarded`/`deferred`, L50-69).
4. `query.rs` `generate_summary` and `export.rs` `format_memory_for_signal` /
   `format_memory_for_handoff`: a `### Suggestions` section. Write the formatting in
   `export_suggestions.rs`, and pay for each call line inside a ledgered function by extraction.
   Lower the ledger entries.
5. Check every other `MemoryEntryType` match or filter the explorer found
   (`fs/memory/persistence.rs::extract_key_notes`, `fs/plan_review/{stages,changes_section,mod}.rs`).
   Leave them unchanged unless a suggestion would be mis-rendered, and list each decision in
   your report.

## Named tests (binding), in `suggestion_tests.rs`

- `suggestion_entries_are_pending_until_resolved`: record a suggestion entry → `pending` lists
  it under `suggestions`; `resolve <id> --outcome implemented --reason "x"` → no longer pending.
- `implemented_outcome_requires_reason`: `resolve <id> --outcome implemented` without a reason
  → error.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib commands::memory::`

## Report

Files changed; exact new counts of the three ledgered functions; the decision for each site in
task 5; the proof result.
