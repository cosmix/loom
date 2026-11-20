# K1 — knowledge check ratchet, tier-2 size limits, cross-file headings, section state, delete-section

Tier: opus (`loom-senior-software-engineer`). Read `../common.md` first, in particular the
"Knowledge check baseline" contract.

## Goal

`loom knowledge check --strict` becomes a gate a distillation stage can pass and can fail: it
fails on issues the stage added and tolerates recorded debt. The catalog also measures what it
exempts today. Evidence: report sections 4.1 (1b) and 4.8 — `--strict` is the most disputed
criterion (knowledge-distill filed 11 of 17 disputes); tier-2 files hold 91% of 18,358 lines and
have no size rule; 398 of 1,173 headings are dated episodes and state is file-level, used on 0
of 117 files; there is no `delete-section`.

## Files you own (write)

- `loom/src/commands/knowledge/check.rs`, `commands/knowledge/mod.rs`,
  `commands/knowledge/annotate.rs`
- `loom/src/fs/knowledge/catalog.rs`, `fs/knowledge/catalog/` (`issue.rs`, `order.rs`,
  `size.rs`), `fs/knowledge/chunker.rs`, `fs/knowledge/dir.rs`, `fs/knowledge/splice.rs`
- `loom/src/cli/types_memory.rs` — you are its only writer. Besides your own knowledge
  arguments, add `#[arg(long)] group: bool` to the memory `Pending` subcommand for K2, with the
  help text "Group pending entries by kind: corrections, mistakes, decisions, other". K2 reads
  that field in the dispatch code it owns.
- tests: existing `commands/knowledge/tests.rs`, `tests_check.rs`, `tests_check_evidence.rs`,
  `tests_annotate.rs`, `tests_replace_section_levels.rs`; new `tests_check_baseline.rs` and
  `tests_delete_section.rs`; tests under `fs/knowledge/`. `tests_eval.rs`, `tests_context.rs` and
  the rest belong to another stage or stay untouched

Do not edit `loom/src/commands/knowledge/eval/**`, `commands/knowledge/context.rs` or
`loom/src/context/**` (another stage owns them).

## Where things are

- `strict_failure_count` — `check.rs:138-148`. `CatalogIssue` — `catalog/issue.rs:31-76`, 10
  variants; `is_review_only` (78-87) is true for `EvidenceChanged`, `EvidenceUnavailable`,
  `UnverifiableReference`. Exhaustive matches a new variant must join: `order.rs` `issue_file`
  (15-27), `issue_kind` (30-42), `issue_payload` (45+); `check.rs` `issue_line` (233-274) and
  `review_issue_line` (276-311). JSON shape: `json_payload` (`check.rs:111-129`).
- CLI `Check` args: `--strict`, `--strict-evidence`, `--json` (`cli/types_memory.rs:118-129`).
- Size rules: `catalog/size.rs` — `MAX_TIER_ONE_SECTION_LINES = 40`, `MAX_TIER_ONE_FILE_LINES =
  250`, `is_tier_one` (54-56), checks at 22-52, no-ops for tier-2.
- `push_duplicate_headings` — `catalog.rs:166-181`, tally keyed by file, so per-file only.
- State: `LifecycleState` (`context/schema/lifecycle.rs:7-19`), stored in frontmatter by
  `annotate`; the chunker takes one state and stamps every chunk (`chunker.rs:29`);
  `KnowledgeChunk.state` is already per chunk. `LifecyclePolicy::Current` admits Active and Draft.
- `replace_section_target` and `splice_section` — `fs/knowledge/dir.rs` (140-147, 278).

## Steps

1. Baseline ratchet per the contract in common.md. Baseline lines are the stable key `order.rs`
   already computes for an issue (kind, file, payload). `--write-baseline <file>` writes the
   current structural set, sorted, and exits 0. Read `loom/tests/maintainability/baseline.rs`
   for the repo's existing baseline file conventions (comments, duplicate-key rejection) and
   follow them; that baseline fails on drift in both directions and yours deliberately does not.
   Put the comparison in `fs/knowledge/catalog/`, not in the CLI handler
   (`doc/loom/knowledge/mistakes/knowledge-cli-invariants.md`).
2. Tier-2 limits from common.md: new constants and checks in `size.rs`, raising the existing
   `OversizedSection` / `OversizedFile` variants. Today that adds 5 files and 5 sections on this
   tree; they are what the baseline is for.
3. Cross-file duplicate headings: new review-only variant `DuplicateHeadingAcrossFiles {
   heading, files }`, built from a heading-to-files map accumulated during `catalog::build`.
   Ignore headings shorter than 12 characters and the generic ones every topic file shares.
4. Section-level state: a line `<!-- state: historical -->` (any `LifecycleState` value)
   directly under a heading sets that section's chunks' state, overriding the file's. Parse it in
   the chunker; it is not part of the chunk body. `loom knowledge annotate <target> --section
   "<heading>" --state <value>` writes or replaces the marker through the same splice path
   `replace-section` uses. Retrieval already filters on `KnowledgeChunk.state`; add a test that a
   historical section is absent under `LifecyclePolicy::Current` and present under `Historical`.
5. `loom knowledge delete-section <file> "<heading>"`: fs-layer function beside
   `replace_section_target`; a non-matching heading is an error, never a silent no-op. Register
   the subcommand where `replace-section` is registered.

## Traps

- `doc/loom/knowledge/mistakes/knowledge-write-channel.md`: knowledge writes from a stage go
  through the CLI under the sandbox grant. Your tests write to temp directories only.
- CRLF files and headings at levels `##` to `######`: `replace-section` handles both; reuse its
  span logic, do not write a second heading matcher.
- `INDEX.md` regenerates on every knowledge write; `delete-section` and the state marker count
  as writes.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib fs::knowledge` and
`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib commands::knowledge` — each
run once.
