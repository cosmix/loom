# Brief: base-run (stage criterion-narrowing-evidence)

## Goal

`loom check <stage> --criterion <n> --at-base` runs the criterion in a confined host job against a checkout of the stage's base commit and records the result as base evidence.

## Files you own

- `loom/src/commands/verify.rs`, `verify_relay.rs`, `loom/src/relay/payload.rs`
- `loom/src/host_verify/mod.rs`, `child.rs`, `child_base.rs` (new), `tests_child_base.rs` (new), `submit.rs`, `wait_state.rs`
- `loom/src/orchestrator/core/inbox_drain/check.rs`
- `loom/src/verify/impact_tests/base.rs`
- `loom/src/stage_cache/mod.rs`, `loom/src/stage_cache/base_checkout.rs` (new)

Frozen contract files are read-only. Files near 400 lines are ledgered exactly in `loom/maintainability-baseline.txt`; report every ledger row that must change.

## Pinned interface (the `narrowing` worker writes these types in `verify/review/handoffs.rs`; compile against them)

```rust
pub struct BaseEvidence { pub index: usize, pub command: String, pub passed: bool,
    pub executed: Option<u64>, pub failed: Option<u64>, pub build_failed: bool, pub timed_out: bool }
pub fn load_base_evidence(work_dir: &Path, stage_id: &str) -> anyhow::Result<Vec<BaseEvidence>>
pub fn save_base_evidence(work_dir: &Path, stage_id: &str, evidence: &BaseEvidence) -> anyhow::Result<()>
```

`save_base_evidence` replaces an entry with the same `index` and `command`.

## CLI and transport

- `CheckArgs` gains `at_base: bool` (`--at-base`); `CheckSelection` gains `AtBase(Vec<usize>)`. `check_selection()` refuses `--at-base` without `--criterion` and with `--contracts`.
- `HostJobRequest` and `JobRecord` gain `at_base: bool` with `#[serde(default)]`. An at-base Check has its own selection identity in `submit.rs` and `wait_state.rs`, like the contracts check the previous stage added.
- A judge session may request it: `inbox_drain/check.rs:81-95` (`admit_session`) already admits `SessionType::Adjudication` for a Check.

## The job (`child_base.rs`)

1. Base commit: `verify::contracts::changes::stage_base(repo, work_dir)` (`changes.rs:29`), the merge base impact tests use.
2. Checkout: a persistent directory per stage and base, `<stage cache root>/base-checkout/<base sha>`, so a second run reuses its build. `stage_cache/base_checkout.rs` provides `fn base_checkout_dir(cache: &StageCache, base: &str) -> PathBuf`; the stage cache root is removed with the stage. Create it as `BaseCheckout::create_in` does (`verify/impact_tests/base.rs:~92`: `git clone --shared --no-checkout` then `git checkout --detach`), without the `Drop` removal: add a constructor that keeps the directory and reuses an existing one whose HEAD is the base and whose tree is clean. A directory in any other state is deleted and recreated. Refuse a checkout path that is a symlink.
3. Run each selected criterion there with the stage's setup, confinement, timeout and stage cache: build the config as `BaseCriteria` does (`base.rs:~54`: `cache_dir: None`, `with_tree_root(Some(root))`) and add `with_stage_cache`. The job is not bound to the worktree's tree digest (`JobTree::differences`, `child.rs:~211`), since it does not run the worktree.
4. Record one `BaseEvidence` per criterion: `command` is the criterion's command text as the stage defines it; `passed`; `executed` and `failed` from the adapter's parsed run when it has them; `build_failed` when the adapter classifies the run as a build failure (`testrun::classify`, `testrun/outcome.rs:15`); `timed_out` on a timeout.
5. Report (`wait_report.rs` is unchanged): outcome label `base:<index>`, summary "at base <sha12>: passed | failed <n> of <m> | selected zero tests | did not build | timed out".

The checkout holds tracked files only; a compiled project builds once per base and is warm afterwards. Say so in the job's first progress line.

## Tests (`tests_child_base.rs`)

Guard launching tests with the confinement probe. Cover: a criterion that fails at base records `passed: false` with counts; one that passes records `passed: true`; a second run reuses the checkout directory; a checkout directory with a foreign HEAD is recreated; `--at-base` without `--criterion` is refused.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib host_verify::`, `--lib verify::impact_tests::` and `--lib commands::verify` pass.
- Report every exhaustive match you touched and every assertion you changed in existing tests.
