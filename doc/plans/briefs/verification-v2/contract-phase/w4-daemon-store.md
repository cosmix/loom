# contract-phase / W4 — freeze record, daemon handler, relay, contract completion check

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D8 (daemon handler and store), D9.
Knowledge: `conventions/dispute-and-adjudication.md` "Dispute File Ownership Convention" and
"Daemon-as-Filesystem-Writer Convention"; `mistakes/sandbox-state-channels.md`. Code:
`loom/src/daemon/server/dispute.rs` (510 lines, `handle_dispute_criteria` L38-213) is the
handler to mirror: `validate_id` before any filesystem access, canonicalised `work_dir`,
per-stage flock, `safe_create_new_in_workdir` on a dirfd.

## Files you own

`loom/src/daemon/protocol.rs` (320), `loom/src/daemon/server/mod.rs`,
`loom/src/daemon/server/contracts.rs` (new), `loom/src/relay/{matrix,kind,payload,tests_matrix}.rs`,
`loom/src/fs/stage_request/{types,apply}.rs` (the spool request and its apply),
`loom/src/orchestrator/core/inbox_drain/{apply,tests_matrix}.rs`,
`loom/src/verify/mod.rs`, `loom/src/verify/contracts/{mod,store,completion}.rs` (new).

## Interfaces you publish (pinned in the other briefs; keep them exact)

```rust
// daemon/protocol.rs
Request::FreezeContracts { auth_token: String, stage_id: String, session_id: String, reports: Vec<ContractRunReport> }
pub struct ContractRunReport { pub contract_id: String, pub adapter: Option<String>, pub outcome: String, pub exit_code: Option<i32> }
Response::ContractsFrozen { files: usize }

// verify/contracts/store.rs
pub struct FreezeRecord { /* DESIGN D8 freeze.json fields */ }
pub fn load_freeze(work_dir: &Path, stage_id: &str) -> Result<Option<FreezeRecord>>;
pub fn frozen_file_path(work_dir: &Path, stage_id: &str, rel: &str) -> PathBuf;
pub fn attempts_spent(work_dir: &Path, stage_id: &str) -> Result<u32>;
pub fn spend_attempt(work_dir: &Path, stage_id: &str) -> Result<u32>;
pub fn write_freeze(work_dir: &Path, record: &FreezeRecord, files: &[(String, Vec<u8>)]) -> Result<()>;

// verify/contracts/completion.rs
pub fn check(stage: &Stage, work_dir: &Path, acceptance_dir: &Path, worktree_root: &Path) -> Result<()>;
```

## Tasks

1. `store.rs`: the `.loom/work/contracts/<stage>/` layout of D8 (`freeze.json`, `files/`,
   `attempts`). Atomic writes. Validate `stage_id` like `validate_id`. Refuse relative paths
   that escape. Attempts are spent when handed out, never derived from an artifact the counted
   operation produces (`conventions/dispute-and-adjudication.md` "Adjudication Attempt Budget
   Convention").
2. `daemon/server/contracts.rs`: `handle_freeze_contracts`. The caller's `session_id` must be the
   stage's current session and a `SessionType::Contract`. Re-check the D8 step-1 path rule with
   read-only git in the stage worktree. Every report must be red (`failed`, `build_failed` or
   `exit_nonzero_unverified`). Hash every contract file and every file matching a `harness`
   glob, and `write_freeze`. Route it in `daemon/server/mod.rs`. Give the request a redacted
   `Debug` like `debug_dispute` (protocol.rs L209).
3. Relay and spool, following every place `Dispute` appears (`rg -ln "StageRequest::Dispute|RequestKind::Dispute" loom/src`):
   `RequestKind::FreezeContracts` (`relay/kind.rs`) with its payload (`relay/payload.rs`),
   `StageRequest::FreezeContracts` (`fs/stage_request/types.rs`) applied by
   `fs/stage_request/apply.rs`, and the matrix tests. For the matrix: in `relay/matrix.rs` add a row for every
   `(SessionType, RequestKind)` pair involving `Contract` or `FreezeContracts`, so `verdict()`'s
   `expect` never panics. `Contract` gets `Stage`'s verdicts except dispute, verdict and
   completion kinds, which it may not send. `FreezeContracts` is allowed only for `Contract`.
   `inbox_drain/apply.rs` applies a relayed freeze through the same handler.
4. `completion.rs`: DESIGN D9. Run contracts through the criteria runner so the certified cache
   applies (`verify/criteria/runner.rs::run_with_cache`, L156-200): build the same `CommandSpec`
   a criterion would. Classify with `testrun::classify`. Messages name
   `loom stage contracts restore <stage>`; dispute-kinds adds the dispute command later.
5. `verify/mod.rs`: `pub mod contracts;`.

## Named tests (binding)

- `freeze_handler_records_hashes_and_copies` (`daemon/server/contracts.rs` tests): a temp work
  dir and worktree, a stage whose session is a Contract session, and one contract file. The
  handler writes `freeze.json` with that file's sha256 and a byte-identical copy under `files/`.
  A second call from a `SessionType::Stage` session is refused.
- `completion_fails_when_frozen_contract_changed` (`verify/contracts/completion.rs` tests): edit
  a frozen file → error naming it and `loom stage contracts restore`.
- `completion_fails_when_contract_not_selected`: a fake adapter run with `executed == Some(0)` →
  error containing `not selected`. Design `check` over an injected runner so this needs no real
  test runner.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib verify::contracts`

## Report

Files changed; the relay rows added (count per kind); exact new counts of any ledgered file
touched (`daemon/server/dispute.rs` must not be touched); the proof result.
