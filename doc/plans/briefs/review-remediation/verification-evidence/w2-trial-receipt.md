# verification-evidence / w2-trial-receipt: a trial receipt binds the tree it ran on and only a settled pass counts

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/verification.md`, section E2.

## Findings covered

- E2: a replacement trial's receipt records HEAD and the replacement hashes only, so an uncommitted edit made after the trial keeps admitting the dispute; a job still `Running` with a passing result already counts as a passed trial; the adjudication prompt shows any receipt of the same bytes as a pass of the current tree.

## Files

Own (write):
- `loom/src/verify/contracts/trial.rs`
- `loom/src/verify/contracts/trial_receipt.rs` (new)
- `loom/src/verify/contracts/trial_receipt_tests.rs`
- `loom/src/verify/contracts/trial_contract_tests.rs` (an earlier stage's contract file; signature updates only)
- `loom/src/host_verify/child_contracts.rs`
- `loom/src/host_verify/tests_child_contracts.rs`
- `loom/src/stage_service/dispute_admission.rs`
- `loom/src/stage_service/dispute_trial_tests.rs`
- `loom/src/orchestrator/adjudication/prompt/contract_replacement.rs`
- `loom/src/orchestrator/adjudication/prompt/contract_replacement_tests.rs`
- `loom/tests/integration/relay_dispute_replacement_fixture.rs` (outside the stage territory; the plan author confirms it; one struct field)

Read only: `loom/src/host_verify/child.rs` (`JobTree::differences`, lines 316-337), `loom/src/host_verify/digest.rs` (`head_in`, `tree_digest_in`, `knowledge_tree_digest_in`), `loom/src/stage_service/dispute_replacement_tests.rs` (fixtures `frozen_stage`, `dispute`, `file`, `assert_refused`, `worktree_file`), `loom/src/orchestrator/adjudication/prompt/contract_tests.rs` (`harness_fixture`, `freeze`, `input_of`), `loom/src/verify/contracts/test_support.rs` (`pinned`).

Frozen (contract tests, never edit): `loom/src/verify/contracts/trial_admission_contract_tests.rs`.

## Changes

### 1. Move the receipt half of `trial.rs` into `trial_receipt.rs`

`trial.rs` is 367 lines and grows. Move, unchanged unless step 2 says otherwise: `RECEIPT_FILE`, `TrialReceipt`, `TrialFile`, `trial_files`, `receipt_covers`, `receipt_files_cover`, `load_receipt`, `passed_trial` (lines 30-60 and 252-359) into `loom/src/verify/contracts/trial_receipt.rs`, with the imports they need. In `trial.rs` declare `#[path = "trial_receipt.rs"] mod receipt;` and `pub use receipt::{current_tree, load_receipt, receipt_covers, receipt_files_cover, trial_files, TrialFile, TrialReceipt, RECEIPT_FILE};`, so every `crate::verify::contracts::trial::...` path keeps working. Keep `#[path = "trial_receipt_tests.rs"] mod tests;` in `trial.rs` (the test module path stays `verify::contracts::trial::tests`); add to `trial_receipt_tests.rs` the imports it used to get through `super::*` (`crate::host_verify::{store as job_store, JobKind, JobRecord, JobState}`, `std::path::Path`, `crate::utils::sha256_hex`) and drop those from `trial.rs` when unused there. Update the module doc of `trial.rs` to name `trial_receipt.rs`.

### 2. The receipt binds the tree digest (`trial_receipt.rs`)

- `TrialReceipt` gains `pub tree_digest: String` (after `head`); it stays `deny_unknown_fields`. A receipt written before this change fails to parse and is skipped by `load_receipt`'s existing warning; no migration.
- `receipt_covers` becomes (pinned):

```rust
pub fn receipt_covers(
    receipt: Option<&TrialReceipt>,
    head: &str,
    tree_digest: &str,
    replacements: &[(String, String)],
) -> Result<(), String>
```

  Checks in this order: no receipt; `receipt.head != head` (existing message); `receipt.tree_digest != tree_digest` with `format!("the passing trial ran on a different tree than the worktree holds now (its uncommitted changes differ): {RUN}")`; then `receipt_files_cover`. `receipt_files_cover` keeps its signature (the prompt uses it, step 5).
- New (pinned):

```rust
/// The HEAD and tree digest of `stage`'s tree read through `git`, as the
/// trial's host job compares them (`JobTree::differences`).
pub fn current_tree(git: &WorktreeGit, stage: &Stage) -> Result<(String, String)>
```

  `head = digest::head_in(git)?`; the digest is `digest::knowledge_tree_digest_in(git)?` when `stage.stage_type == StageType::Knowledge`, else `digest::tree_digest_in(git)?`.
- `passed_trial`: require `JobState::Passed`; drop `JobState::Running`. Rewrite its doc comment: only a job the runner settled as passed counts; `loom check` returns only on a terminal state (`host_verify::wait`), so a filing after it sees the settled record.

### 3. The child saves the digest (`loom/src/host_verify/child_contracts.rs`, `save_receipt`, lines 256-269)

Set `tree_digest: record.tree_digest.clone()`. `run_contract_trial` saves the receipt only when `tree.differences(record)` found none (line 168-171), so the record's digest is the tree the trial ran on.

### 4. Admission compares the digest (`loom/src/stage_service/dispute_admission.rs`, `check_trial`, lines 230-252)

Replace the `head_in` call with `let (head, tree_digest) = current_tree(&git, stage)?;` (same pinned `git`) and call `receipt_covers(receipt.as_ref(), &head, &tree_digest, &files)`. Update the doc comment: a passing trial of the same bytes on the worktree's current HEAD and tree. Remove the `digest` import if it becomes unused.

### 5. The judge sees whether the pass is current (`contract_replacement.rs`, `trial_line`, lines 60-80)

Compute the current state: `input.worktree` → `WorktreeGit::pinned_in_project_of(input.work_dir, root)` → `current_tree(&git, input.stage)`, as an `Option` (any error or no worktree gives `None`). Then:
- receipt whose files cover the filing and `receipt_covers(Some(&receipt), head, digest, &pairs)` is `Ok` against the current state: the existing `Trial: passed in a confined host job at {head} (format and static gates, then the contracts)\n\n` line;
- receipt whose files cover the filing but the current check fails or the state is `None`: `Trial: a trial of these bytes passed at {head}, but the stage's tree no longer matches the one it ran on, so it is no evidence for the tree as it is now\n\n`;
- otherwise the existing `Trial: no matching trial receipt is still recorded\n\n`.
Update the doc comment of `trial_line` to describe the three lines.

### 6. Integration fixture (`loom/tests/integration/relay_dispute_replacement_fixture.rs`, `record_trial`, lines 198-242)

Add `tree_digest: digest::tree_digest_in(&git).expect("read the worktree digest")` to the `TrialReceipt` literal (the module already imports `digest`). Nothing else.

## Interfaces shared with other workers

None. `current_tree` and the new `receipt_covers` are used only by the files above and by this stage's contract file.

## Tests

- `loom/src/stage_service/dispute_trial_tests.rs` (module `stage_service::dispute_kinds::replacement_tests::trial_tests`): replace `worktree_head` with `worktree_state(work_dir) -> (String, String)` returning `current_tree(&pinned_git, &stage)` for the stage worktree (load the stage with `load_stage(STAGE, work_dir)`); `record_trial` takes `(head, tree_digest)` and writes the digest into both the `JobRecord` and the receipt. Existing tests keep their assertions; `a_trial_at_another_head_does_not_admit_the_replacement` passes the real digest with the zero head. Add `an_edit_after_the_trial_does_not_admit_the_replacement`: `frozen_stage()`, record a trial of `EDITED` at the current state, then write `src/extra.rs` (untracked, not frozen) into the worktree (`worktree_file(&work_dir, "src/extra.rs")`), then `assert_refused(&work_dir, dispute(Some(EDITED), None), &["different tree"])`. Fails today: the filing is accepted.
- `loom/src/verify/contracts/trial_receipt_tests.rs`: the `receipt` helper and both literal receipts gain `tree_digest`; `receipt_covers` calls pass a digest equal to the receipt's. Add `a_running_trial_with_a_passing_result_is_no_receipt`: `job(work, 1, true, JobState::Running, true, &bytes("running"))` gives `load_receipt == None`. Add `a_receipt_of_another_tree_does_not_cover`: same head and files, other digest, error contains `different tree`. Both fail today (the second does not compile today; that counts).
- `loom/src/verify/contracts/trial_contract_tests.rs`, `a_filing_needs_a_passing_trial`: add `tree_digest` to the literal and the digest argument to each `receipt_covers` call; no assertion changes.
- `loom/src/host_verify/tests_child_contracts.rs`, `a_passing_trial_records_its_receipt` (line 248): after `fx.run`, assert `load_receipt(..) == None` while the record is still `Running`, then settle it as the runner would (`let mut done = record.clone(); done.state = JobState::Passed; store::save_current(&fx.work_dir, &done)`), then keep the existing assertions and add `assert_eq!(receipt.tree_digest, record.tree_digest)`.
- `contract_replacement_tests.rs` (module `orchestrator::adjudication::prompt::contract::replacement_tests`): `the_proposed_replacement_names_the_trial_that_passed` moves to `super::tests::harness_fixture()` (a real registered worktree) with the replacement request kind set as `fixture_with` does, and `save_passing_trial` takes `(head, tree_digest)` read with `current_tree(&pinned(&fx.worktree), &fx.stage)`; the expected line names the real head. Add `a_trial_of_an_older_tree_is_not_shown_as_current`: same setup, then write an untracked file into `fx.worktree`; the evidence contains `but the stage's tree no longer matches` and not `Trial: passed`. Update `a_receipt_of_other_bytes_is_no_trial_of_this_filing` to the new `save_passing_trial` signature.
- Contracts this worker turns green: `dirty-tree-change-voids-trial`, `unsettled-trial-is-no-receipt`.

## Proof

From `loom/`:

```bash
cargo check --all-targets
cargo test --lib verify::contracts::trial
cargo test --lib stage_service::dispute_kinds
cargo test --lib orchestrator::adjudication::prompt
cargo test --lib host_verify::tests_child_contracts
cargo test --test integration relay_dispute_replacement
```

## Traps

- Read the digest through the same pinned `git` that reads HEAD; `digest::tree_digest(path)` (discovered git) would follow the worktree's own `.git` file.
- Keep the HEAD check before the digest check, so `a_trial_at_another_head_does_not_admit_the_replacement` keeps its message.
- `with_overlay` and the restore code stay in `trial.rs`; move only the receipt items listed in step 1.
- A test fixture that keeps writing `tree_digest: "digest"` into a receipt now makes every filing fail with `different tree`; compute the real digest.
- Report every existing test you changed (they are listed in the stage's INTEGRITY part).
