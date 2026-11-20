# Brief: filing (stage contract-dispute-filing)

## Goal

One `dispute-contract` filing names several contracts and carries replacements keyed by frozen file. Admission refuses a filing while any frozen file has drifted in the worktree.

## Files you own

- `loom/src/cli/types_stage_disputes.rs`
- `loom/src/commands/stage/dispute_kinds.rs`, `dispute_transport.rs`, `dispute_replacement_tests.rs`
- `loom/src/models/dispute.rs`, `loom/src/models/dispute_replacement.rs`, `loom/src/models/stage/dispute_budgets.rs`
- `loom/src/stage_service/dispute_admission.rs`, `dispute_kinds.rs`, `dispute_replacement_tests.rs`, `dispute_race_tests.rs`
- `loom/src/verify/contracts/drift.rs` (new), `completion.rs`, `completion_tests.rs`, `mod.rs`
- `loom/src/daemon/**`, `loom/src/fs/stage_request/**`, `loom/tests/integration/**` (tests and fixtures that build a contract dispute)

Frozen contract files are read-only. `models/dispute.rs` (378 lines) and other files near 400 lines are ledgered exactly in `loom/maintainability-baseline.txt`: put new types in `models/dispute_replacement.rs` or a new file, and report every ledger row that must change.

## Model (pinned; write this first, two other workers compile against it)

```rust
// models/dispute.rs, serde tag "kind", kebab-case as today
DisputeKind::Contract {
    contract_ids: Vec<String>,                 // was contract_id: String
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    replacements: Vec<FileReplacement>,        // was replacement + replacement_base_sha256
}
// models/dispute_replacement.rs
pub struct FileReplacement {
    pub file: String,                          // the path form FreezeRecord.files[].path uses
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha256: Option<String>,           // set by admission, any client value overwritten
}
pub struct ContractRuling { pub contract: String, pub ruling: ContractRulingKind,
                            pub reasoning: String, pub citations: Vec<Citation> }
pub enum ContractRulingKind { Accept, Reject }         // serde kebab-case
DisputeVerdict::ContractRulings { rulings: Vec<ContractRuling>, plan_patch: PlanPatch }
                                                       // serde tag value "contract-rulings"
```

No migration and no serde alias for the old shape (project rule). Re-export `ContractRuling`, `ContractRulingKind` and `FileReplacement` from `models::dispute`.

## CLI

- `DisputeContractArgs` (`types_stage_disputes.rs:176-199`): `contract: Vec<String>` with `#[arg(long = "contract", required = true, num_args = 1..)]`, as `--finding` is declared at `:163`; `replacement: Vec<String>` with `num_args = 0..`, each value `<frozen-file>=<path>`. Update `From<DisputeContractArgs> for DisputeFiling` (`:231-243`) and `DisputeTarget::Contract` (`dispute_kinds.rs:30-43`).
- `snapshot` (`dispute_kinds.rs:68-90`): parse each `file=path`, refuse a value without `=`, an empty side or a repeated file; read each path with `read_replacement` (`:115-152`), which keeps its rules (outside the worktree, no symlink, UTF-8, `MAX_REPLACEMENT_BYTES`).
- `ensure_fits_ticket` (`:158-192`) measures the whole kind. When it does not fit, the message says to file the replacements in separate filings, one file each.
- `dispute_transport.rs`: `replacement_note` (`:96`), `subject` (`:200`) and `wording` (`:150`) name every contract and every replaced file.

## Drift

`verify/contracts/drift.rs`:

```rust
pub struct Drift { pub path: String, pub kind: DriftKind }
pub enum DriftKind { Changed, Missing, Unreadable(String) }
pub fn frozen_drift(record: &FreezeRecord, acceptance_dir: &Path, worktree_root: &Path)
    -> anyhow::Result<Vec<Drift>>
```

Move the logic of the private `changed_frozen_files` (`completion.rs:219-241`) here; `completion.rs` calls `frozen_drift` and renders the same three messages, so `completion_tests.rs` keeps passing. Only files of `record.files` count: a new file matching a harness glob is not drift.

## Admission (`stage_service/dispute_admission.rs::confirm_contract`, `:87-135`)

Refuse, with a message that names the fix, before the budget is touched (it is spent at `stage_service/dispute_kinds.rs:268`):

1. an empty `contract_ids`, or a repeated id;
2. an id that is not a frozen contract of the stage (the existing check, per id);
3. a replacement whose `file` is not in the freeze record's `files`;
4. a replacement for a file that holds a contract, while a frozen contract with that file is not in `contract_ids`;
5. a replacement over `MAX_REPLACEMENT_BYTES`, or identical to the frozen content (existing checks, per file);
6. any `frozen_drift` entry: "frozen file '<path>' changed in the worktree: restore it with `loom stage contracts restore <stage>` and file the change as `--replacement <path>=<file>`". Read the worktree site as `stage_service/contracts.rs:355` does.

Then set each replacement's `base_sha256` from the record. One filing spends one unit of `contract_disputes` (`dispute_budgets.rs:62`, `:97`).

## Tests

Update every fixture and test that builds `DisputeKind::Contract`: `rg -n 'DisputeKind::Contract|contract_id|replacement_base_sha256' loom/src loom/tests`. Add admission tests for refusals 1, 3, 4 and 6, and one for a two-contract, two-file filing that spends one unit. `loom/tests/integration/relay_dispute_replacement_{e2e,fixture}.rs` keep their near-limit cases with the new flag form.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib stage_service::`, `--lib commands::stage::`, `--lib verify::contracts::` and `--lib models::` pass.
- Report: every match on `DisputeKind::Contract` you changed, every one you left for the other two workers (they own `orchestrator/adjudication/**` and `orchestrator/signals/**`), and the assertions you changed in existing tests.
