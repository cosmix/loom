# Brief: narrowing (stage criterion-narrowing-evidence)

## Goal

The adjudicator narrows or deletes an acceptance criterion only when loom's own base run shows it fails at the stage's base, and every such amendment leaves a hand-off record.

## Files you own

- `loom/src/orchestrator/adjudication/verdict.rs`, `verdict_narrowing.rs` (new), `plan_patch.rs`, `plan_patch_tests.rs`, `apply_accept.rs`, `apply_accept_tests.rs`, `apply_contract_rulings.rs`, `mod.rs`
- `loom/src/orchestrator/adjudication/prompt/**`
- `loom/src/verify/review/handoffs.rs` (new), `verdict_records.rs`, `mod.rs`

Frozen contract files are read-only. `verdict.rs` is at the 400-line limit and ledgered exactly in `loom/maintainability-baseline.txt`: logic goes in `verdict_narrowing.rs`; report every ledger row that must change.

## Types (pinned; write `verify/review/handoffs.rs` first, two other workers compile against it)

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BaseEvidence { pub index: usize, pub command: String, pub passed: bool,
    pub executed: Option<u64>, pub failed: Option<u64>, pub build_failed: bool, pub timed_out: bool }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Handoff { pub stage: String, pub field: String, pub index: usize, pub tests: Vec<String>,
    pub runs_at: String, pub reason: String, pub dispute: String, pub verified: bool }
pub fn load_base_evidence(work_dir: &Path, stage_id: &str) -> anyhow::Result<Vec<BaseEvidence>>
pub fn save_base_evidence(work_dir: &Path, stage_id: &str, evidence: &BaseEvidence) -> anyhow::Result<()>
pub fn load_handoffs(work_dir: &Path, stage_id: &str) -> anyhow::Result<Vec<Handoff>>
pub fn record_handoffs(work_dir: &Path, stage_id: &str, handoffs: &[Handoff]) -> anyhow::Result<()>
```

Stores: `.loom/work/reviews/<stage>/base-evidence.json` and `handoffs.json`, written through the locked read-modify-write of `verify/review/verdict_records.rs::update` (`:~83`), versioned and `deny_unknown_fields` like `Carried` (`verify/review/store.rs:117-127`). `record_handoffs` is idempotent on `(dispute, field, index)`; `save_base_evidence` replaces an entry with the same `index` and `command`.

## Validation (`verdict_narrowing.rs`)

```rust
pub fn check_narrowing(stage_id: &str, dispute: &str, plan_patch: &serde_json::Value,
    current: &[serde_json::Value], evidence: &[BaseEvidence]) -> Result<Vec<Handoff>, String>
```

`plan_patch`: `{"field","patches":[{"op","index","value"}],"leaves_out":[{"index","tests":[..],"runs_at","reason"}]}`; `leaves_out` optional. `current`: the stage's entries of that field as JSON (a string, or an object with `command`). Evidence applies to an entry when its `index` matches and its `command` equals the current entry's command. `Err` (text the judge reads) when:

| Case | Text contains |
| --- | --- |
| a `replace` value equals `current[index]` | "changes nothing" |
| a `delete` has no `leaves_out` entry for its index | "leaves_out" |
| a `leaves_out` entry has no applicable evidence | "--at-base" |
| the evidence passed | "passes at the base" |
| the evidence has `build_failed`, or `executed == Some(0)` | "the stage's own" |
| `failed == Some(n)` and `n` is less than the entry's `tests` count | "fails only" |

`Ok`: one `Handoff` per entry, `verified` true when the evidence has a `failed` count, false for a timed-out or unparsed base run. An entry with an empty `tests` list or an empty `runs_at` is an `Err`.

Route: `parse_and_validate_in` (`verdict.rs:83`) sends a `DisputeKind::Criterion` accept through a stage-aware branch after `validate_accept` (`:228`): it loads `current` from the stage for the patched field and `evidence` with `load_base_evidence`, and calls `check_narrowing`. An `Err` reaches the judge unrecorded (`record.rs:105-117`). `plan_patch::normalize` and `canonical_patches_inner` (`plan_patch.rs:~49`, `:~139`) must keep `leaves_out` through canonicalization; `AmendmentPatch` (`plan/amendment.rs:~97`) is unchanged, since `leaves_out` is a sibling of `patches`.

Apply (`apply_accept.rs:~29`): after the amendments apply, recompute the hand-offs with `check_narrowing` and `record_handoffs`. A contract deletion (a `delete` patch in a `contract-rulings` verdict, `apply_contract_rulings.rs`) appends a hand-off too: `field: "contracts"`, `tests` the contract's id, `reason` its scenario and the judge's reasoning, `verified: false`.

## Prompt (`prompt/criterion.rs`, shared helpers in `criterion_entry.rs`)

Add to the verdict rules, in both briefings:

- To replace a criterion with one that selects fewer tests, or to delete it, first run `loom check <stage> --criterion <n> --at-base` and read its `Host job:` line.
- Narrow only when the criterion fails there. Name every test the amendment leaves out in `plan_patch.leaves_out` with where it will run (`integration-verify`, `ci`, or `nowhere`) and why. An amendment that changes which tests run and declares no `leaves_out` is invalid.
- A criterion that passes at base, selects no test there or does not build there fails because of the stage's own change or its own new test. Do not narrow it and do not reject: answer `needs-more-evidence` telling the agent to fix its test, or, for a frozen contract, to file `loom stage dispute-contract` with a replacement.
- Skipping a test is for a test that cannot pass whatever the implementation. A flaky or slow test is not that: say what would fix it.

Show the JSON with `leaves_out` in `schema_json`. The whole acceptance briefing is pinned by `prompt/tests_golden.rs:50` (`GOLDEN`, `:~96`): regenerate the constant and read the diff line by line; nothing outside the new rules and schema may move.

## Tests

`plan_patch_tests.rs`: `leaves_out` survives canonicalization. `apply_accept_tests.rs`: an accepted narrowing appends one hand-off; a replay appends none. Record-time: a narrowing without evidence is refused and records nothing.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::adjudication::` and `--lib verify::review::` pass.
- Report every assertion you changed in existing tests, the golden included.
