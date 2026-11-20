# Brief: gate (stage contract-check-and-trial)

## Goal

A replacement is filed only after a passing trial; the judge sees the trial result and every assertion the replacement drops; the freeze refuses an environment red; agents and contract writers are told how contracts run.

## Files you own

- `loom/src/stage_service/dispute_admission.rs`, `dispute_replacement_tests.rs`
- `loom/src/verify/integrity/mod.rs`, `lost.rs` (new)
- `loom/src/verify/criteria/environment.rs` (new), `loom/src/verify/criteria/mod.rs`
- `loom/src/commands/plan/verify/dry_run_classify.rs`
- `loom/src/verify/contracts/freeze.rs`
- `loom/src/orchestrator/adjudication/prompt/**`
- `loom/src/orchestrator/signals/v2_section.rs`, `v2_section_tests.rs`, `loom/src/orchestrator/signals/contract/rules.rs`
- `README.md`, `skills/**`

Frozen contract files are read-only. Files near 400 lines are ledgered exactly in `loom/maintainability-baseline.txt`; report every ledger row that must change.

## Interface you compile against (the `check-job` worker writes `verify/contracts/trial.rs`)

```rust
pub struct TrialReceipt { pub head: String, pub files: Vec<TrialFile> }
pub struct TrialFile { pub path: String, pub sha256: String }
pub fn receipt_covers(receipt: Option<&TrialReceipt>, head: &str,
    replacements: &[(String, String)]) -> Result<(), String>
pub fn load_receipt(work_dir: &Path, stage_id: &str) -> anyhow::Result<Option<TrialReceipt>>
```

## 1. Receipt at admission (`dispute_admission.rs::confirm_contract`)

After the checks the previous stage added, when the filing has replacements: read the worktree HEAD, call `load_receipt` and `receipt_covers` with each replacement's file and the sha256 of its content, and refuse on `Err` with its text. Nothing is spent on a refusal. A filing without replacements needs no receipt. Tests in `dispute_replacement_tests.rs`: no receipt, a receipt for other bytes, a receipt for another head, a matching receipt.

## 2. Lost statements (`verify/integrity/lost.rs`, re-exported from `mod.rs`)

```rust
pub fn lost_statements(path: &str, before: &str, after: &str) -> Vec<String>
```

Use the detector the integrity gate uses: the language profile for `path`, `statements_of(content, assertion, profile)` (`verify/integrity/edits.rs:77`) and `unmatched(side, other)` (`:86`), both `pub(super)`. Return the statements of `before` unmatched in `after`, in source order; an extension without a profile gives an empty list.

Prompt (`prompt/contract_replacement.rs::sections`, `:59`): after each replaced file's diff add "### Assertions the replacement drops or changes", listing `lost_statements(file, frozen, replacement)` or "none". Add to the verdict semantics: for each listed statement cite what in the replacement covers the same behaviour, or reject the contracts of that file. Add the trial line under "## Proposed replacement": "Trial: passed in a confined host job at <head> (format and static gates, then the contracts)"; admission guarantees it exists.

## 3. Environment reds (`verify/criteria/environment.rs`)

```rust
pub fn environment_marker(stdout: &str, stderr: &str) -> Option<&'static str>
```

Move `ENVIRONMENT_MARKERS` and the matching of `environment_marker` (`commands/plan/verify/dry_run_classify.rs:34-44`, `:85-90`) here; the dry run calls it and its private `environment_marker` is removed. The list is unchanged (it already holds "operation not permitted").

Freeze (`verify/contracts/freeze.rs::judge_run`, `:84`): a red (`Failed`, or unparsed with a non-zero exit) whose output has a marker is refused: "contract '<id>' fails for an environment reason (<marker>), so its red proves nothing about the implementation: the test must run inside loom's confinement (no network, no nested OS sandbox, writes only in the tree and TMPDIR)". `BuildFailed` stays an accepted red. The refusal goes through `refuse` (`freeze.rs:~262`) like the others.

## 4. Text

- Contract writer (`signals/contract/rules.rs`, `WRITING_RULES`, `:6-24`): add a numbered rule: "Every contract runs inside loom's confinement at freeze and at completion: no network, writes only inside the worktree and TMPDIR, and no nested OS sandbox, so a test that starts sandbox-exec, bwrap or a confined loom command cannot start there. Assert on what the code generates (arguments, profile, environment) and do not launch it. The freeze refuses a red caused by the environment."
- Stage signal (`signals/v2_section.rs::append_frozen_contracts`): add: "Run `loom check <stage> --contracts` as soon as the contract tests compile, before the rest of the work. A contract that fails there for a reason no implementation can change is disputed at once: copy the frozen file to `$LOOM_SCRATCH_DIR`, edit the copy so the test checks the same scenario in a form that runs under confinement, run `loom check <stage> --contracts --replacement <frozen-file>=<path>`, and file ONE `loom stage dispute-contract` naming every affected contract when the trial passes." Update `v2_section_tests.rs`.
- `README.md` (the `loom check` and dispute sections), `skills/loom-orchestration/SKILL.md` ("Plan version 2 stages": contracts and what `loom check` runs), `skills/loom-plan-writer/references/v2-contracts.md` (Sections 3 and 5: the run environment rule for contracts, the trial before a replacement).

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib verify::integrity::`, `--lib verify::criteria::environment`, `--lib verify::contracts::`, `--lib stage_service::`, `--lib orchestrator::adjudication::prompt` and `--lib orchestrator::signals::` pass.
- Report every assertion you changed in existing tests.
