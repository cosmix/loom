# T3: the findings ledger and its import (trace-core, wave 2)

Read `../common.md` "Findings ledger" in full; it is your specification.

## Files you own

`loom/src/planning/findings.rs` (+ `findings/` for submodules and tests),
`loom/src/fs/plan_lifecycle.rs` (+ `plan_lifecycle/`). T1 adds `pub mod findings;` to
`loom/src/planning/mod.rs`.

## Tasks

1. **Parse.** `pub fn parse(text: &str) -> Ledger { pub findings: Vec<Finding>, pub problems:
   Vec<LedgerProblem> }` with `Finding` and `FindingStatus` as common.md gives them. Use
   `sections_of` for headings so fenced headings are ignored. Problems: `findings-bad-entry` (a
   level-2 heading that is not `F-<n> <title>`), `findings-duplicate-id`, `findings-bad-status`.
2. **Render helpers.** `pub fn entry_text(id, title, about: &[String], source: Option<&str>,
   status: &FindingStatus, text) -> String`, `pub fn set_status(ledger_text, id, status) ->
   Result<String>` (rewrites or inserts the entry's Status line only; errors on an unknown id),
   `pub fn next_id(ledger: &Ledger) -> String`, and the sanitizers for titles, paragraphs and
   reasons (common.md "Untrusted text"; cut on char boundaries with `char_indices`, never byte
   slicing).
3. **Import.** `pub fn import_review(root: &Path, plan_rel: &Path, review_text: &str) ->
   anyhow::Result<usize>`. The feature comes from the plan path (`kinds::classify` gives
   `DocKind::Plan { feature: Some(f), .. }`). The plan's Implements tokens come from
   `implements::parse` over the plan file's text (read with the bounded no-follow reader the
   planning module uses, `fs::safe_read`). Items come from `run_review::review_items` for
   `OpenFindings` and `OpenQuestions`; the section anchor in `Source:` is the GitHub anchor of
   that section's heading. Skip items whose `Source:` line already exists. Write after the symlink check on every path component from the
   repository root to `findings.md`, under `fs::locking::lock_plan(<findings path>)`, writing with
   `fs::safe_replace::replace_no_follow` or `restore_no_follow` (common.md). Read the plan text
   with `fs::safe_read::read_to_string_bounded(root, plan_rel, <limit>)`.
4. **Lifecycle hook.** In `mark_plan_done_if_all_merged`, after `plan_review::generate` returns
   the review path, read it and call `crate::planning::findings::import_review(&root,
   <plan path relative to root>, &text)` with context ("Failed to import run review findings").
   `get_plan_source_path` returns the configured path, which may be relative
   (`doc/plans/PLAN-x.md`): use it as is when relative, else strip `root`
   (`fs::resolve_source_path` resolves the absolute form if needed).

## Tests

Unit tests: parse round trip of `entry_text`, status rewrite leaving other entries byte for byte,
unknown status reads open with a problem, a reason with control characters, a title of 100
multi-byte chars, import into an existing ledger keeps its entries, an item whose text changed is
a new entry, and a lifecycle test (follow the existing tests of `mark_plan_done_if_all_merged`)
proving the ledger is written once when completion runs twice. The four ledger contracts in
`loom/tests/planning_trace_contracts.rs` are frozen: make them pass without editing them.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::findings fs::plan_lifecycle::` and
the ledger contracts pass; no file over 400 lines.
