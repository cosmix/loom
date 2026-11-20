# Brief: check-job (stage contract-check-and-trial)

## Goal

`loom check <stage> --contracts` runs the stage's frozen contracts in a confined Check host job. With `--replacement <frozen-file>=<path>` it trials replacement files there and records a receipt when they pass.

## Files you own

- `loom/src/commands/verify.rs`, `verify_relay.rs`, `loom/src/relay/payload.rs`
- `loom/src/host_verify/mod.rs`, `child.rs`, `child_contracts.rs` (new), `submit.rs`, `wait_state.rs`, `wait_report.rs`, `tests_child_contracts.rs` (new)
- `loom/src/orchestrator/core/inbox_drain/check.rs`
- `loom/src/verify/contracts/trial.rs` (new), `completion.rs`, `completion_tests.rs`, `mod.rs`

Frozen contract files are read-only. `host_verify/child.rs` is 387 lines and ledgered exactly in `loom/maintainability-baseline.txt`: the new job body lives in `child_contracts.rs`; report every ledger row that must change.

## Pinned interface (the `gate` worker compiles against `trial.rs`; write it first)

```rust
// commands/verify.rs
CheckArgs { .., pub contracts: bool /* --contracts */, pub replacement: Vec<String> /* --replacement f=p */ }
pub enum CheckSelection { All, Criteria(Vec<usize>), Contracts, Trial(Vec<(String, PathBuf)>) }
pub fn check_selection(args: &CheckArgs) -> anyhow::Result<CheckSelection>
// (a private `selection(criteria: &[usize])` already exists at verify.rs:108; keep or fold it in)
// --contracts with --criterion: error. --replacement without --contracts: error.
// A --replacement value without '=' or with an empty side: error.

// verify/contracts/trial.rs
pub fn with_overlay<T>(worktree_root: &Path, files: &[(String, Vec<u8>)],
    run: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T>
pub struct TrialReceipt { pub head: String, pub files: Vec<TrialFile> }   // serde
pub struct TrialFile { pub path: String, pub sha256: String }
pub fn receipt_covers(receipt: Option<&TrialReceipt>, head: &str,
    replacements: &[(String, String)]) -> Result<(), String>
pub fn load_receipt(work_dir: &Path, stage_id: &str) -> anyhow::Result<Option<TrialReceipt>>
```

- `with_overlay`: for each `(relative path, bytes)`, refuse a missing file, a symlinked target or a symlinked parent before writing anything; save the earlier bytes and mode; write with `fs/safe_replace.rs::restore_no_follow` (the writer `apply_contract_replacement.rs:132` uses); call `run`; restore every file's bytes and mode whether `run` returned `Ok` or `Err`, and on unwind (a drop guard); return `run`'s result. A restore failure is an error that names the file.
- `receipt_covers`: `Err` when the receipt is `None`, its head differs, or a `(file, sha256)` pair is not among its files; the text contains "loom check" and "--replacement".
- `load_receipt` reads the newest passing trial's receipt for the stage from the job store.

## The job

- Transport: `HostJobRequest` (`relay/payload.rs:52`) and `JobRecord` (`host_verify/mod.rs:166`) gain `contracts: bool` with `#[serde(default)]` (both are `deny_unknown_fields`). Replacement bytes travel in the request as `replacements: Vec<FileReplacement>` (`models::dispute::FileReplacement`, `#[serde(default)]`), bounded like a dispute: 32 KiB each, the whole ticket within `MAX_TICKET_BYTES`; the CLI reads each path with the rules of `commands/stage/dispute_kinds.rs::read_replacement` (`:115-152`; call it, do not copy it) and refuses a request that does not fit before sending. The daemon saves them as job files (`store::save_job_file`, `host_verify/store.rs:200`), not in `JobRecord`.
- Submit rules (`submit.rs:82`, `:119`, `:183-185`, `:264`) and the request-state name (`wait_state.rs:33`): a contracts or trial selection is a Check with its own selection identity, so it neither joins nor is joined by a criteria Check. `inbox_drain/check.rs:27-49` passes the new fields through.
- Child (`child.rs:45-51`, `:96-109`): `JobKind::Check` with `contracts` set runs `child_contracts::run`:
  1. no replacements: `verify::contracts::completion::check(stage, work_dir, acceptance_dir, worktree_root)`; one outcome per contract labelled `contract:<id>`.
  2. with replacements: refuse a file the freeze record lacks; bind HEAD and tree digest first (`JobTree`, `child.rs:~190-211`); then `with_overlay(worktree_root, files, || ..)` running, inside the overlay, the freeze's format and static gates for the replaced files (`verify/contracts/freeze.rs::check_formatting`, `:~198`, with `written` = the replaced paths) and the contract check with the replaced files' expected hashes taken from the replacement bytes. After the overlay returns, on a pass save `trial.json` (the `TrialReceipt`: HEAD and each file's sha256) as a job file. Re-check the tree digest after the restore; a difference fails the job.
  - `completion::check` compares every frozen file with its recorded hash (`completion.rs:60-90`): add a variant that takes the expected hash per overridden file, used only by the trial.
- A full `loom check <stage>` (selection `All`) on a `standard` stage with a freeze record runs step 1 after the acceptance criteria and goal-backward checks, so the gate an agent runs before `loom stage complete` shows contract failures.
- `wait_report.rs:46-108`: the heading names a contracts check or a trial; a passing trial prints "Trial passed: file the dispute with the same --replacement values."

## Completion failure text (`completion.rs:77-86`, pinned at `completion_tests.rs:121`)

Rewrite the guidance: a failing contract needs an implementation that passes it; when it fails for a reason no implementation can change (the test needs something the confined runner lacks), write a replacement, run `loom check <stage> --contracts --replacement <frozen-file>=<path>`, and file one `loom stage dispute-contract <stage> --contract <id> [--contract <id> ...] --replacement <frozen-file>=<path> --reason ...`. When a failing contract's output carries an environment marker (`crate::verify::criteria::environment::environment_marker`, written by the `gate` worker), add one line naming the marker; it is advice and changes no result.

## Tests (`tests_child_contracts.rs`)

Guard each test that launches with `confinement_unavailable`-style probing (`tests_child.rs:25`). Cover: a contracts check reports each contract; a failing trial leaves the worktree byte-identical and records no receipt; a passing trial records the receipt; a trial for a file outside the record is refused; `--contracts` does not run acceptance criteria.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib host_verify::`, `--lib verify::contracts::` and `--lib commands::verify` pass.
- Report every exhaustive match on the job or request shape you touched, and every assertion you changed in existing tests.
