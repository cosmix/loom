# Brief: verdict-apply (stage contract-dispute-filing)

## Goal

A contract dispute is ruled per contract. A ruling that changes nothing is refused when the verdict is recorded and returned to the judge. Accepted replacements are written per file, then one re-freeze.

## Files you own

- `loom/src/orchestrator/adjudication/verdict.rs`, `verdict_kinds.rs`, `verdict_kinds_tests.rs`, `verdict_contract.rs` (new), `verdict_hold_contract_tests.rs`, `record.rs`
- `loom/src/orchestrator/adjudication/apply_kinds.rs`, `apply_kinds_tests.rs`, `apply_contract.rs`, `apply_contract_tests.rs`, `apply_contract_choice_tests.rs`, `apply_contract_replacement.rs`, `apply_contract_replacement_tests.rs`, `apply_contract_rulings.rs` (new), `feedback.rs`, `mod.rs`
- `loom/src/commands/stage/adjudicate.rs`

Frozen contract files are read-only. `verdict.rs` (385 lines) and `apply_contract.rs` (304) are ledgered exactly in `loom/maintainability-baseline.txt`: new logic goes in the two new files; report every ledger row that must change.

## Model (pinned; the `filing` worker writes it in `models/`, compile against this)

```rust
DisputeKind::Contract { contract_ids: Vec<String>, replacements: Vec<FileReplacement> }
FileReplacement { file: String, content: String, base_sha256: Option<String> }
ContractRuling { contract: String, ruling: ContractRulingKind, reasoning: String, citations: Vec<Citation> }
ContractRulingKind { Accept, Reject }
DisputeVerdict::ContractRulings { rulings: Vec<ContractRuling>, plan_patch: PlanPatch }   // tag "contract-rulings"
```

Until that worker returns, build with a local stub of the model if you must; do not edit `models/`.

## Validation (`verdict_contract.rs`)

```rust
pub fn validate_contract_rulings(json: &serde_json::Value, disputed: &[String],
    replaced: &BTreeSet<String>, contract_index: &BTreeMap<String, usize>)
    -> Result<Vec<ContractRuling>, String>
```

- Verdict JSON: `{"verdict":"contract-rulings","rulings":[{"contract","ruling","reasoning","citations"}],"plan_patch":{"field":"contracts","patches":[{"op","index","value"}]}}`; `plan_patch` optional, normalized with `plan_patch::normalize` and `contracts_patch` (`verdict_kinds.rs:94`).
- `Err` (text the judge reads) when: a disputed id has no ruling or two; a ruling names an undisputed id; a ruling has no reasoning; an `accept` names a contract outside `replaced` with no patch at its index (text contains "changes nothing" and the id); a patch index is not the index of a disputed contract ruled accept (text contains "outside"). Follow `check_coverage` (`verdict_kinds.rs:182`) for coverage.
- Route: `parse_and_validate_in` (`verdict.rs:83-94`) sends `DisputeKind::Contract` to a stage-aware branch, as it sends `FreezeRefusal` to `verdict_freeze_refusal::validate`. It builds `disputed` from the kind, `replaced` from the kind's replacements and the freeze record (a disputed contract whose frozen file is replaced), and `contract_index` from the stage's `contracts`. An `Err` there reaches the judge as "The verdict ... was not recorded: {problem}" (`record.rs:105-117`) and spends no evidence round; that is why this is not coerced to needs-more-evidence. `needs-more-evidence` for a contract dispute stays as it is. A plain `accept` or `reject` verdict on a contract dispute is an `Err` that names the `contract-rulings` shape.
- `commands/stage/adjudicate.rs`: the relay notice and pre-check (`:96`, `:135-141`) accept the new verdict tag.

## Apply (`apply_contract_rulings.rs`, called from `apply_kinds.rs:77-80`)

```rust
pub fn files_to_write(rulings: &[ContractRuling], contract_files: &BTreeMap<String, String>,
    replacements: &[String]) -> Vec<String>
```

A file holding at least one contract is written when every contract in it is ruled accept; a file holding none (harness) when every ruling is an accept; order follows `replacements`.

Sequence, under the locks `apply_contract::accept` already holds (`apply_contract.rs:53-79`):

1. No ruling is an accept: write `reject_notice` naming every contract and its reasoning, then `notify_and_requeue`. Nothing else.
2. Conflict check per file to write (`apply_contract_replacement.rs::conflict`, `:65-97`, generalised from the contract's own file to any recorded file): the frozen hash equals the replacement's `base_sha256` or `sha256(content)`. Any other value escalates as today and writes nothing.
3. `amend_plan` with the verdict's patches (`plan_patch::build_amendment_requests` already orders several).
4. Write each file with `write` (`:132`, `restore_no_follow`, mode kept), generalised to any recorded path.
5. One `read_frozen_set` and `refreeze_set` (`apply_contract.rs:116-124`).
6. Notice: each contract's outcome and reasoning, the files written (the agent stages and commits them), the files not written and why. It no longer ends "implement until every contract passes" for a filing where nothing was written.

A replay after a crash finds some files already equal to their replacement; step 2 admits them and step 4 rewrites the same bytes. Keep the `FAIL_AFTER_WRITE` seam (`:55`) and add a test that fails after the first of two writes and replays.

`feedback.rs::evidence_action` (`:86-99`) and `apply_kinds.rs::apply_rejection` (`:129-141`) name every contract id and the `--replacement <frozen-file>=<path>` form.

## Tests

Rewrite the fixtures in `apply_contract_tests.rs` (`:54` `Fixture`), `apply_contract_replacement_tests.rs` and `verdict_kinds_tests.rs` (`:12`, `:93`) to the new shapes. Add: mixed rulings over two files; a harness replacement with one reject; the two-write crash replay; a plain `accept` refused.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::adjudication::` passes.
- Report every assertion you changed in existing tests, and any behaviour of the old single-contract path you could not keep.
