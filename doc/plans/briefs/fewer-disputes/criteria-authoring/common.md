# criteria-authoring: rules every worker follows

Read this file in full, then your own brief. The plan is `doc/plans/PLAN-fewer-disputes.md`
(`IN_PROGRESS-PLAN-fewer-disputes.md` once it runs). Its "Decisions" section is authoritative where a
brief and the plan differ; the stage description's PUBLIC SURFACE is authoritative for every signature the
frozen contracts call; this file is authoritative for every internal signature below.

## Where you are

- The stage runs with `working_dir: loom`. Paths in briefs are relative to `loom/` unless they start with
  `../` (the repository root: `../skills/`, `../README.md`).
- The sibling plans `doc/plans/PLAN-autonomous-recovery.md` and `doc/plans/PLAN-live-test-lane.md` edit some
  of the same files and may have merged before this stage runs. Locate every seam by symbol first
  (`loom map --outline <file>`, `loom map --find-all <symbol>`); line numbers in briefs are advisory. A named
  symbol that moved or a widened enum with a new arm: use the merged name and record
  `loom memory note "found: <what moved> (<file:line>)"`. Never reintroduce anything a sibling removed.
- Knowledge to read first (by heading; `loom knowledge context --query "<heading>" --budget-tokens 2500`
  returns it quoted): `architecture/contract-phase.md` "Where a Contract Runs", "Refused Freezes Are
  Disputed", "Accepted Dispute Re-Freeze"; `architecture/plan-lifecycle-and-fields.md` "Adding New Plan Fields
  Checklist", "Freeze-Time Formatter Gate", "Plan Version 2: Fields, Validation, and `plan verify` Lints";
  `architecture/test-runner-adapters.md` "Run-Root Markers".

## The design in six lines

1. `ContractSpec.test` becomes `Option<String>`, a hint. The contract writer names every contract's test with
   `loom stage contracts freeze <id> --test <contract>=<identifier>` and, only where the planner left
   `runner` or `cwd` unset, `--runner <contract>=<adapter>` and `--cwd <contract>=<dir>`. Each flag repeats.
2. The daemon validates the choices when it admits the freeze request, persists them in
   `.loom/work/contracts/<stage>/choices.json` (a file only the daemon writes, beside `freeze.json`), and the
   freeze job and the freeze application read the stage through `choices::load_effective_stage`, which
   replaces `stage.contracts` with the plan's specs overlaid with the choices. Nothing in the `Stage` record
   changes, so `check_definition_hash` (`handoff/completion/identity.rs`) is unaffected. The writer's own CLI
   pre-check (`check_site_changes`, which derives scaffolds from each contract's test) reads the saved choices
   read-only through `choices::overlay_with_saved` and overlays the flags of this call on them, so a later
   freeze keeps the flags it does not repeat. A second freeze request whose choices differ from the saved ones
   is refused while a freeze job for the same head and tree is non-terminal: that job would be joined and its
   binding (`contracts_digest`) would break.
3. The freeze record's `FrozenContract` gains `test`. After the freeze every reader overlays the record on
   the plan's spec with `store::effective_contract(record, spec)`: completion, `contracts show`, the
   implementer's frozen-contracts table, the adjudication prompts and the dispute re-freeze.
4. The freeze job's format gate widens into a static gate over every acceptance and `after_stage` criterion
   that is a pure check. A failing pure check refuses the freeze only when its output names a file the writer
   wrote; compiler-backed lints (`cargo clippy`, `tsc --noEmit`, `go vet`, `golangci-lint run`) run only when
   every contract compiled (every report outcome is `failed`, none `build_failed` or unverified), because a
   red contract that fails to build makes them report the missing API in the contract file. The gate's own
   writes (a lint cache such as `tsconfig.tsbuildinfo`) are removed before the binding is checked again (W2:
   `host_verify/freeze_format.rs`), so they cannot move the tree digest. Such a static
   refusal (a failed host job whose only red outcome is `format`) is returned to the live contract writer:
   `loom stage contracts freeze` waits for the host job and then for the daemon's application of it (W1a;
   `host_verify/wait.rs` follows application for a freeze as it does for a completion), the writer reads the
   failing output in `Host job: failed` and a marker line that says whether the refusal was returned (`n of 3`)
   or filed, fixes its files and freezes again, and no dispute is filed. The exact words every brief and the
   doctrine use: a static refusal is returned to the writer; it is filed as a `freeze-refusal` dispute on the
   third static refusal in the session, or when the writer stays quiet 5 minutes after a refusal (W1b: the count
   is `DaemonRefusal.returned` in `refused.json`; `refusal::record_static_refusal`, `STATIC_RETURN_LIMIT` = 3;
   the tick files a returned record once the writer is quiet). Every other failed freeze job is filed at once.
5. `loom plan verify --dry-run` runs the plan's `loom.provision` entries in one shared checkout of HEAD, then
   every acceptance, `after_stage` and `wiring_tests` command there under the stage's confinement, and
   classifies each result. Exit 126 or 127 is an error only for a stage with no dependencies (a stage with
   dependencies may get the program or script from an earlier stage, so it is a note there). A criterion that
   passes at base is a note; it is a warning only when every acceptance criterion of a stage with no
   dependencies passes, so `--strict --dry-run` passes on a plan with sound criteria.
6. Plan-time: a `wiring` pattern shaped like a definition is a lint error in v2 (warning in v1);
   `consumer_deferred` and a non-empty `reachable` satisfy the cross-stage coverage warning (the rule a version 2
   plan meets under `--strict`), and `consumer_deferred` also the leaf rule (which only version 1 plans meet: a
   version 2 standard stage is never a leaf, `validation.rs` leaf rule and `v2_lints/bookends.rs`); an extended
   acceptance criterion defaults to 300 s.

## Internal signatures (exact; every worker codes against these)

```rust
// plan/schema/types_v2.rs (W0), re-exported from crate::plan::schema
pub struct ContractChoice { pub test: Option<String>, pub runner: Option<String>, pub cwd: Option<String> }
pub type ContractChoices = std::collections::BTreeMap<String, ContractChoice>;
impl ContractSpec {
    /// test: the choice's, else the hint. runner and cwd: the planner's, else the choice's.
    pub fn with_choice(&self, choice: &ContractChoice) -> ContractSpec;
    /// The test identifier, or an error naming the contract when none is set.
    pub fn required_test(&self) -> anyhow::Result<&str>;
}

// verify/contracts/record_view.rs (W0), re-exported from verify::contracts::store
pub fn recorded_cwd<'a>(record: &'a FreezeRecord, contract: &ContractSpec) -> Option<&'a str>; // moved, unchanged
pub fn effective_contract(record: &FreezeRecord, spec: &ContractSpec) -> ContractSpec;

// verify/contracts/choices.rs (W1b), `pub mod choices` in verify/contracts/mod.rs (W0 declares it)
pub fn parse_choices(tests: &[String], runners: &[String], cwds: &[String]) -> anyhow::Result<ContractChoices>;
pub fn merge_choices(current: &ContractChoices, update: &ContractChoices) -> ContractChoices;
/// The plan's specs overlaid with the saved choices merged with `update`; read-only, never fails for a missing
/// test (it may stay `None`). The writer's CLI pre-check and `lenient_contracts` use it.
pub fn overlay_with_saved(work_dir: &Path, stage_id: &str, contracts: &[ContractSpec],
    update: &ContractChoices) -> anyhow::Result<Vec<ContractSpec>>;
pub fn validate_choices(contracts: &[ContractSpec], choices: &ContractChoices,
    worktree_root: &Path, working_dir: &Path, base: &str) -> anyhow::Result<()>;
pub fn load_choices(work_dir: &Path, stage_id: &str) -> anyhow::Result<ContractChoices>; // empty when absent
pub fn save_choices(work_dir: &Path, stage_id: &str, choices: &ContractChoices) -> anyhow::Result<()>;
pub fn effective_contracts(work_dir: &Path, stage: &Stage) -> anyhow::Result<Vec<ContractSpec>>; // bails when a test is missing
pub fn load_effective_stage(stage_id: &str, work_dir: &Path) -> anyhow::Result<Stage>;
pub fn lenient_contracts(work_dir: &Path, stage: &Stage) -> Vec<ContractSpec>; // overlay, never fails; test may stay None

// verify/contracts/format_gate.rs (W2)
pub fn is_pure_check(command: &str, cwd: &Path) -> bool;
pub fn runs_at_freeze(command: &str, cwd: &Path, contracts_compile: bool) -> bool;
pub fn attribute_failure(output: &str, written: &[String]) -> Vec<String>;

// verify/contracts/freeze.rs (W2)
pub(crate) fn written_paths(site: &ContractSite, repo: &WorktreeGit) -> Result<Vec<String>>;
pub(crate) fn check_formatting(site: &ContractSite, confinement: CommandConfinement,
    written: &[String], contracts_compile: bool) -> Result<()>;
// host_verify/freeze_format.rs (W2): lists the untracked paths (host_verify/digest.rs `untracked_in`, W2) before
// the gate and removes the ones the gate created afterwards, also when the gate refuses
pub(super) fn format_outcome(site: &ContractSite, git: &WorktreeGit, confinement: CommandConfinement,
    written: &[String], contracts_compile: bool) -> Result<CriterionOutcome>;

// stage_service/contracts.rs (W1b): one parameter appended
pub(crate) fn handle_freeze_contracts(work_dir: &Path, stage_id: &str, session_id: &str, head: &str,
    tree_digest: &str, request_id: &str, choices: &ContractChoices) -> Result<Response>;

// verify/criteria/config.rs (W4), re-exported from verify::criteria
pub const DEFAULT_EXTENDED_CRITERION_TIMEOUT: Duration = Duration::from_secs(300);
pub fn extended_criterion_timeout(check: &TruthCheck) -> Duration;

// plan/schema/types.rs (W0)
impl StageDefinition { pub fn has_wiring_coverage(&self) -> bool; } // wiring || wiring_tests || reachable non-empty
```

The wire field on the freeze request is `choices: ContractChoices`, `#[serde(default, skip_serializing_if =
"BTreeMap::is_empty")]`, on `StageRequest::FreezeContracts` (`stage_service/types.rs`) and
`Request::FreezeContracts` (`daemon/protocol.rs`).

## Ownership

- You write only the files your row of the stage's worker table owns. A compile error in a file no row owns
  goes to the row whose change caused it; record the file with `loom memory note`.
- Two workers never write one file. If you need a change in another worker's file, stop and report the exact
  change instead of making it.
- Never run git, never write under `.loom/` or `~/.claude/`, never spawn subagents.
- Waves: (1) W0, W0b, W4 and W5 in ONE message. (2) W1a, W1b, W2 and W3 in ONE message, each fixing the
  compile errors W0 leaves in its own files. The crate does not compile between the waves; that is expected.

## The maintainability ledger is exact (read before touching a listed file)

`cargo test --test maintainability` compares `maintainability-baseline.txt` to the tree and fails when a listed
file or function GROWS OR SHRINKS. The ledger is a ratchet file: never edit it. These files and functions
are listed and are touched by this stage, so each edit must leave the line count exactly as it was:

| Entry | Lines | How to keep it |
| --- | --- | --- |
| `src/models/stage/types.rs` | 591 | the new `Stage` field adds 3 lines; shorten existing doc comments in the same file by 3 lines |
| `src/models/stage/methods.rs` | 492 | the `from_definition` copy adds 1 line; remove one blank or comment line in the file |
| `src/models/stage/defaults.rs` `default` | 76 | pair two empty-list fields on one line, as the literal's comment says (`#[rustfmt::skip]` is already there) |
| `src/plan/schema/validation.rs` | 1063 | edits replace lines one for one (briefs W2) |
| `src/plan/schema/validation.rs` `validate` | 357 | same |
| `src/plan/schema/validation.rs` `check_cross_stage_wiring_coverage` | 66 | same |

Every new file stays under 400 lines and every new function under 50 (the same test enforces both). A file
that would cross 400 splits into a sibling module or moves its `#[cfg(test)]` block into a `*_tests.rs` file
wired with `#[path]`.

## Code rules

- A value that arrives from an agent, a model verdict or the wire is untrusted: validate ids with
  `crate::validation::validate_id` before any path join, cut text with
  `crate::models::dispute::truncate_to_byte_limit` (never a byte slice), strip control characters with
  `crate::models::dispute::strip_control`.
- A refusal the daemon returns to an agent is `Response::Error { message }`, never an `Err` (the spool drain
  retries an `Err` forever). `Err` is for a failure to queue a job or write a file.
- Errors: `anyhow` with `.context(...)` at the boundary, typed errors only where a caller classifies by type.
- No new dependency, no new configuration key, no placeholder, stub or `todo!()`.
- Doc comments state current behaviour. No dated history, no "was", no "used to", no attribution, none of
  the words in the style table of CLAUDE.md Rule 19.
- Every existing-stage mutation goes through `crate::verify::transitions::update_stage`; slow work runs
  outside the closure.
- Atomic file writes under the contracts directory use `crate::fs::locking::{atomic_write_locked,
  locked_dir_update}` and bounded reads use `crate::fs::safe_read::read_bounded`, as `store.rs` does for
  `attempts`.

## Tests

- In-crate tests live in a `*_tests.rs` file beside the module (`#[cfg(test)] #[path = "x_tests.rs"] mod
  tests;`). Tests that set process environment variables are `#[serial_test::serial]` and restore them.
- Git fixtures: `git init -b main` in a `TempDir`, local `user.email` and `user.name`, and the environment
  isolation `tests/completion_gates_contracts.rs::git` uses (`GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM`
  pointed at absent files, `GIT_CONFIG_NOSYSTEM=1`).
- A test that needs bubblewrap guards itself with
  `crate::process::sandbox_probe::skip_unless(crate::process::sandbox_probe::confined_commands_runnable(),
  "<test name>", "<why>")` (it skips in your session sandbox, which cannot nest bubblewrap). Compile-check it;
  `loom check` runs it. The frozen contracts run inside the host job's confinement, where bubblewrap cannot
  nest: a contract that skips for lack of confinement passes vacuously, so no frozen contract may depend on
  it. The dry run therefore takes its command executor as a parameter (`run_dry_run_with`, brief W4) and the
  contracts pass a recording executor; only an in-crate test in `dry_run_tests.rs` runs the real confined one.
- Frozen contract files (`tests/writer_choices_contracts.rs`, `tests/freeze_static_gate_contracts.rs`,
  `tests/plan_lints_contracts.rs`, `tests/dry_run_contracts.rs`) and their harness
  (`tests/criteria_authoring_contracts/**`) are never edited. `loom stage contracts show criteria-authoring`
  lists them.
- Existing assertions change only where the new type forces it (`Some(..)` around a `test` value, a new
  field in a literal). Never delete or weaken an assertion; list every changed assertion line in your
  report (the orchestrator files one `dispute-integrity`).

## Checks

You run at most ONE narrowly scoped check over the files you wrote (for example `cargo test --lib
<your module>::`), once; wave 1 workers whose crate cannot compile yet run none. The orchestrator runs
`loom check criteria-authoring`.
