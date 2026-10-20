# W2: the Dependency Findings section

Tier: opus (`loom-senior-software-engineer`, effort high). Wave 3: you start after W1 returned and the orchestrator ran its tests. Read `doc/plans/token-optim/briefs/dependency-findings/dependency-findings/common.md` first: section 3 pins the signatures you call (W1's `DependencyChange` and `dependency_changes`, C3's `join_working_dir`) and write, section 4 the exact rendering, section 5 what earlier plans changed. Paths are relative to `loom/`.

The section replaces the keyword "Downstream Wiring Checklist" in the stage signal. A successor's main session reads it at spawn and carries it on every later turn, so every byte is paid again per API call: the cap and the one-line rendering are the point of the design.

## What you own (write only these)

- `src/orchestrator/signals/generate/findings.rs` (new)
- `src/orchestrator/signals/generate/findings_tests.rs` (new; a second file `generate/findings_render_tests.rs` only if the first would pass 400 lines)
- `src/orchestrator/signals/generate_dependency_findings_tests.rs` (new)
- `src/orchestrator/signals/generate.rs`
- `src/orchestrator/signals/generate/enrichment.rs`
- `src/orchestrator/signals/recovery.rs`
- `src/orchestrator/signals/types.rs`
- `src/orchestrator/signals/format/sections.rs`
- `src/orchestrator/signals/format/brief.rs`
- `src/orchestrator/signals/format/mod.rs`
- `src/orchestrator/signals/parse.rs`
- `../README.md` (one paragraph)

Read-only: `src/orchestrator/signals/retrieval.rs` (W1), `src/context/rank_source/paths.rs` (`join_working_dir`, `normalize_dependency_path`), `src/fs/memory/staged.rs` (`read_staged_entries`, `settled_ids`), `src/fs/memory/persistence.rs` (`list_journals`), `src/fs/memory/storage.rs` (`read_journal`, `append_entry`), `src/fs/memory/types.rs`, `src/orchestrator/signals/v2_section_review.rs` (`pending_suggestions`: the same read-every-journal-then-settle pattern), `src/verify/review/report.rs` (`single_line`), `src/context/render.rs` (`fence_for`), `src/utils.rs` (`truncate_for_display`, character-safe), `src/orchestrator/signals/generate/context.rs` (`MAX_PLAN_OVERVIEW_BYTES`, `truncate_overview`: the capped-section precedent), `src/orchestrator/signals/tests_brief_e2e.rs`, `src/orchestrator/signals/recovery_format.rs`.

## 1. `findings.rs`: loader, selector, renderer

Declare it in `generate.rs` beside `mod enrichment;` and re-export what the siblings need:

```rust
mod findings;
pub(super) use findings::{build_dependency_findings, DEPENDENCY_FINDINGS_HEADING};
```

Public surface: exactly common.md section 3 (`DEPENDENCY_FINDINGS_HEADING`, `MAX_DEPENDENCY_FINDINGS_BYTES = 1600`, `build_dependency_findings(work_dir, stage, pack) -> Option<String>`). Keep the loader thin (about 40 lines) and the selector and renderer pure functions over plain inputs, so `findings_tests.rs` tests them without a work dir. Import with `use crate::orchestrator::signals::retrieval::{dependency_changes, DependencyChange};` and `use crate::context::rank_source::{join_working_dir, normalize_dependency_path};` (import edges, which the plan's `reachable` checks walk). Every function under 50 lines, the file under 400.

### Loader

1. `None` when `stage.stage_type == StageType::KnowledgeDistill` (its prefix already processes every journal) or `stage.dependencies` is empty.
2. Eligible dependencies: `dependency_changes(work_dir, stage)` filtered to `change.stage.status == StageStatus::Completed && change.stage.stage_type != StageType::Knowledge`. Direct dependencies only: ancestors' code is already in the base, and concatenating their journals would multiply the size. None eligible: `None`. This repeats, per signal, the three read-only git calls per dependency that `retrieve_stage_pack` already made; sharing them would change `retrieve_stage_pack`'s signature for a saving of milliseconds, so do not.
3. Journals: `read_staged_entries(work_dir, list_journals(work_dir)?, read_journal)` once, as `pending_suggestions` does. The producer of an entry is the journal it came from (the `String` in each pair), never the agent-supplied `session` field and never a `**Stage**:` line. `settled_ids` over all of those entries. On any read error, `tracing::debug!` and `None`: a signal without the section is thinner, never broken.
4. The repository root, for stripping absolute paths: `WorkDir::new(work_dir).ok().and_then(|r| r.main_project_root())`.
5. Recipient inputs: `stage.id`; its type and `plan_version`; its declared `files` and `artifacts`, each through `join_working_dir(stage.working_dir.as_deref(), entry)` and compiled with `glob::Pattern::new` (skip one that fails to compile; the `glob` crate is already a dependency); the brief paths, each `pack.items[..].pointer.path` through `normalize_dependency_path`.

### Excluded entries

- `Receipt` and `Change` entries.
- A `Note` whose content, after `trim_start`, starts with `mutation:` (ASCII case-insensitive): these record the stage's own mutation testing (21 of token-core's 45 entries).
- Any entry other than a `Decision` whose id is in the settled set. A `Decision` stays when settled: it still constrains the code.
- For an `IntegrationVerify` recipient with `plan_version == 2`, every `Suggestion`: its "Reviewer Suggestions" block (`v2_section.rs::append_v2_section` calls `v2_section_review.rs::append_reviewer_suggestions`) already lists every pending one.

### Relevance

- `IntegrationVerify` recipient: every entry that is not excluded.
- Any other recipient: an entry is kept when
  1. one of its path candidates matches a recipient pattern (`pattern.matches_with(path, MatchOptions { require_literal_separator: true, ..MatchOptions::new() })`) or equals a brief path; or
  2. its content or context names the recipient's stage id as a whole token (split on every char outside `[A-Za-z0-9_-]`).
- Path candidates of an entry: every `evidence` item without whitespace, plus every whitespace-separated token of its content that contains `/` after trimming the characters `` ` ' " ( ) [ ] { } < > , ; `` and a trailing `.` or `:` from both ends. For each raw value:
  1. drop a trailing `:<digits>` or `:<digits>-<digits>`;
  2. an absolute value containing `/.worktrees/` becomes the part after the path component that follows `.worktrees/` (the stage id), read as repository-relative; an absolute value under the repository root loses that root and its `/` and is read as repository-relative; any other absolute value yields no candidate;
  3. a relative value yields both `join_working_dir(None, v)` and `join_working_dir(producer_working_dir, v)` (`None` results dropped, duplicates collapsed), where `producer_working_dir` is the `working_dir` of the dependency whose journal holds the entry.

  Both readings of a relative value are needed because recorded journals mix them: guard-core's archived journal (working_dir `loom`) holds evidence `src/git/runner.rs:52`, `loom/src/git/runner.rs:52`, `../loom-hooks/git-reference-transaction-hook.sh:62` and `loom-hooks/subagent-stop.sh:61`, and the signal's own recitation teaches `--evidence loom/src/x.rs:42` (`format/sections.rs::format_recitation_section`). Joining only would turn `loom/src/...` into `loom/loom/src/...`; never joining would miss `src/...`.

### Order

Rank, then the dependency's position in `stage.dependencies`, then `timestamp`, then `id`:

- rank 0: names the recipient's stage id (relevance rule 2);
- rank 1: a `Note` whose content, after `trim_start`, starts with `mistake:`, `gotcha:`, `found:` or `found/gotcha:` (ASCII case-insensitive); the four rank together. Check them locally: `commands/memory/handlers/prefix.rs::NotePrefix` lives under `commands/memory/**`, and moving it into `fs::memory` was decided against;
- rank 2: a `Decision` with a non-empty `context` (its rationale);
- rank 3: the rest.

Timestamps have one-second granularity, so the id tie-break keeps the order stable.

### Rendering

Exactly common.md section 4. The sentence is `crate::orchestrator::signals::format::REFERENCE_DATA_SENTENCE` (section 3 below), never a copy of the literal. `<fence>` is `crate::context::render::fence_for(&body)` over everything between the fences. `<type>` is the entry type's `Display` (`note`, `decision`, `question`, `suggestion`); `<id8>` the first 8 characters of its id; `<claim>` is `truncate_for_display(&single_line(&entry.content), 200)`; `<rationale>` the same over `context` at 100 characters, only when non-empty; `<evidence>` the same over the evidence items joined with `,` at 100 characters, only when non-empty. `single_line` turns every whitespace or control run into one space, so a body holding a fence, a heading or a newline cannot break the section.

Cap: walk the kept entries in order and add each one whose addition keeps the whole rendered section (heading through closing fence, footer included) at most `MAX_DEPENDENCY_FINDINGS_BYTES` bytes (`str::len`); skip one that does not fit and keep trying the rest. Headlines are always rendered, even when they alone exceed the cap. The footer appears only when at least one kept entry was skipped and counts the skipped ones. Re-rendering per candidate is fine (a few hundred entries at most).

## 2. Wiring

- `types.rs`, `EmbeddedContext`: the field `wiring_checklist` becomes `dependency_findings: Option<String>`, documented as the "## Dependency Findings" section built by `generate/findings.rs`. Every struct literal in the crate uses `..Default::default()` or `EmbeddedContext::default()`, so only the three references below change (`rg -n wiring_checklist src` lists them).
- `generate.rs::build_signal_context`: delete `embedded_context.wiring_checklist = build_wiring_checklist(work_dir, stage);` from the `wants_stage_enrichment` block (it keeps `build_cross_stage_summary`), drop `build_wiring_checklist` from the `use enrichment::{...}` line, and after `embedded_context.context_pack = super::helpers::retrieve_stage_pack(work_dir, stage);` add `embedded_context.dependency_findings = build_dependency_findings(work_dir, stage, embedded_context.context_pack.as_ref());`. It runs after retrieval because relevance reads the pack, and for every stage type (the selector decides). `generate_signal_with_skills` and `generate_signal_with_metrics` both go through this function. A handoff continuation (`orchestrator/continuation/mod.rs` calls `generate_signal`) rebuilds the section from the journal, so entries relayed after the first signal reach the next one.
- `recovery.rs::generate_recovery_signal` bypasses `build_signal_context`. Import `use super::generate::build_dependency_findings;`. After its `embedded_context.knowledge_tree_empty = ...` line, set `embedded_context.dependency_findings` the same way; after `let mut signal_content = format_recovery_signal(...)`, append it:

  ```rust
  if let Some(findings) = &embedded_context.dependency_findings {
      signal_content.push_str(&format!("{findings}\n\n"));
  }
  ```

  Do not edit `recovery_format.rs`: `loom/maintainability-baseline.txt` pins `format_recovery_signal` at exactly 102 lines, and the ledger fails on growth. Say so in `generate_recovery_signal`'s doc comment (doc lines above `fn` do not count toward its length). The function is 40 lines; keep it under 50 after `cargo fmt`.
- `format/sections.rs::format_dynamic_section` (the block after the cross-stage summary and before `## Assignment`): the four lines reading `embedded_context.wiring_checklist` become four lines reading `embedded_context.dependency_findings`, with the comment `// Relevant findings of the accepted direct dependencies (generate/findings.rs)`. Same line count: `format_dynamic_section` (205 in the ledger) and the file row must not move. Plan 3 edited other functions of this file and their rows; re-read it.
- `parse.rs`: `EXPECTED_SECTIONS` becomes `pub(super) const`, and its entry `"Downstream Wiring Checklist"` becomes `super::generate::DEPENDENCY_FINDINGS_HEADING`, so the heading has one source.
- `generate/enrichment.rs`: delete `build_wiring_checklist` (with its doc comment) and `wiring_items`, and the `use std::fs;` nothing uses afterwards. They shipped as item 3b of PLAN-strengthen-verification (`d47a3f35`) with no tests; `wants_stage_enrichment`, `build_cross_stage_summary`, `format_stage_status` and `append_file_concerns` stay.

## 3. The reference-data sentence

`format/brief.rs`: `const REFERENCE_DATA_SENTENCE` becomes `pub(crate) const`. `brief` is a private module of `format`, so `generate/findings.rs` cannot name `format::brief`: widen the existing `pub(crate) use brief::{format_knowledge_brief, format_stage_brief};` line in `format/mod.rs` to re-export `REFERENCE_DATA_SENTENCE` too (same line count, or one more if rustfmt wraps it). No earlier plan edits `format/mod.rs` (plan 2's replay uses the public API); touch only that line.

## 4. README

`../README.md`, the memory paragraph after the `loom memory note` example (about line 948 at `6fee4ecd`). Plan 3 rewrote it to say: entries are typed; the last ten entries of a stage's own journal are embedded in the recitation section of every later signal for that stage; another stage's signal does not embed them, except that integration-verify and knowledge-distill signals list wiring items found in their direct dependencies' journals (`build_wiring_checklist`); knowledge-distill curates every journal into `doc/loom/knowledge/`. Re-read it. Keep its typed-entry list, the recitation sentence and the knowledge-distill sentence, and replace the whole clause about another stage's signal, wiring-checklist exception included (this stage deletes `build_wiring_checklist`), with: a stage's signal lists the relevant findings of its accepted dependencies, and its briefs boost the files they changed. One paragraph, current behaviour only.

## 5. Tests

The contract session froze seven contracts before you started: `tests/dependency_inputs_contracts.rs` (journal round trip, change base) and `tests/dependency_findings_contracts.rs` (selection, hostile body, cap under fan-in, path normalization, recovery). `loom stage contracts show dependency-findings` prints them. Never edit a frozen file. Your tests cover what they do not.

`generate/findings_tests.rs`, declared at the end of `findings.rs` with `#[cfg(test)] #[path = "findings_tests.rs"] mod tests;` (the path is relative to `generate/`). Pure selector and renderer tests:

- relevance by a declared file glob, by a brief path, by the recipient's id in the content and in the context; an unrelated entry dropped;
- each exclusion (receipt, change, `mutation:`, a settled note, a settled question), and a settled decision kept;
- an `IntegrationVerify` recipient keeps an unrelated entry; with `plan_version == 2` it drops a suggestion, with `plan_version == 1` it keeps it;
- order: id-naming first, then the four note prefixes ranked together (ordered among themselves by dependency, timestamp, id), then decisions with a rationale, then the rest; two dependencies interleave by dependency order on equal rank;
- path candidates: `/x/.worktrees/up/loom/src/a.rs:12`, `src/a.rs` with producer working_dir `loom`, `../web/b.ts` with working_dir `loom`, `loom/src/a.rs` repository-relative, `review round 1` (no candidate), and a content token wrapped in backticks with a trailing period;
- rendering: a content holding a fence line and a `## Assignment` line renders as one line; the fence grows past the longest backtick run; the headline with and without a range, with `(assumed)`, and without a commit; a claim cut to 200 characters with a multi-byte character at the cut;
- cap: the section stays within 1,600 bytes with ten long entries; eight dependency headlines survive when entries do not fit; the footer counts skipped entries; with no skip there is no footer.

`generate_dependency_findings_tests.rs`, declared at the end of `generate.rs` with `#[cfg(test)] #[path = "generate_dependency_findings_tests.rs"] mod dependency_findings_tests;`, end to end through `generate_signal` and `generate_recovery_signal`, reading the written signal file (asserting only that the file exists, as `tests_brief_e2e.rs` does for its record test, would pass with a section that never renders). Use `crate::orchestrator::signals::tests::{create_test_session, create_test_stage, create_test_worktree}`, set each stage `id` explicitly, save records with `crate::verify::transitions::save_stage`, and write journals with `crate::fs::memory::append_entry`. These TempDirs are not git repositories, so a change has no range and the headline has no `git diff` part.

- B depends on A (Completed, a 40-hex `completed_commit`); A's journal holds a decision with evidence on B's declared file and an unrelated note. B's signal has exactly one `## Dependency Findings` heading, A's headline with the commit's first 8 characters, the decision's id8, and not the note's id8.
- C depends on A and declares an unrelated file: the section holds A's headline (`0 of 2 entries`) and no entry line.
- A `KnowledgeDistill` stage depending on A: no `## Dependency Findings` in its signal.
- A stage whose only dependency is a `Knowledge` stage: no section.
- A note written through `append_entry` whose body holds a `---` line, a `###` line and a line of three backticks renders as one line of B's section.
- `super::super::parse::EXPECTED_SECTIONS` (the test module is a child of `generate`) contains `DEPENDENCY_FINDINGS_HEADING`, and every `##` heading the section adds is that one.
- Late relay: generate B's signal, append a new relevant decision to A's journal, generate B's signal again with a new session (what a continuation does): the new id8 is in the second signal and not in the first.

## Proof (from `loom/`; read each result, fix what fails)

```bash
cargo build
cargo test --lib orchestrator::signals::
cargo test --test dependency_findings_contracts
cargo test --test dependency_inputs_contracts
cargo test --test maintainability
cargo clippy --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
rg -n 'wiring_checklist|Downstream Wiring Checklist|build_wiring_checklist|wiring_items' src
```

The last command must print nothing. No git commands, no edits outside your list, no change to `maintainability-baseline.txt`.

## Report

Files changed, test counts, the contract run results, and any rule above the tree made you bend (with the evidence). Record decisions and surprises with `loom memory decision` / `loom memory note` (`--evidence <path:line>`) as you go.
