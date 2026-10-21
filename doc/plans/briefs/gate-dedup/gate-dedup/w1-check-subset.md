# W1: `loom check --criterion` and Host-Job Reuse

Tier: opus. Files owned are listed in the stage's worker table. Read `common.md` first.

## Goal

1. A fix round re-runs only the criteria that failed, locally and as a host job.
2. A Complete job that follows a passing Check job on the same tree reuses every cached pass.

## Steps

1. **Runner selection.**
   - Add `only: Option<BTreeSet<usize>>` to `CriteriaConfig` (`verify/criteria/config.rs`), with a builder `with_only(...)` matching the existing `with_cache_dir` style.
   - In `verify/criteria/runner.rs`, the loop `for (index, criterion) in stage.acceptance.iter().enumerate()` skips an index outside `only`. It keeps the original index for output, and fails when `only` names an index `>= len`.
   - A skipped criterion is neither run nor stored nor reported as passed.
   - Tests in the existing runner test module: `only` runs exactly the named criteria and keeps their labels; an out-of-range index errors.
2. **CLI.**
   - `Commands::Check` gains `#[arg(long = "criterion", value_name = "INDEX")] criterion: Vec<usize>` (`cli/types.rs:290`).
   - `run_check` (`cli/dispatch.rs:190`) passes it on. Keep `dispatch` line-neutral: the arm destructures on its existing line.
   - `commands/verify.rs` `execute` takes the selection; keep `execute` at 63 lines by moving any new logic into a helper.
   - The local path builds the config with `with_only`. The summary line names `subset: <indices>` when a selection is set.
   - `commands/stage/acceptance_runner.rs` is yours if the display path needs the selection.
3. **Host jobs.**
   - Add `criteria: Option<Vec<usize>>` to `SubmitRequest`, `JobRecord` and the relay `HostJobRequest`.
   - `commands/verify_relay.rs` fills it from the CLI.
   - `covers` (`host_verify/submit.rs:105-110`): a job covers a request only when the job's selection is `None`, or is a superset of the request's selection. A request with `None` is covered only by a job with `None`.
   - The child (`host_verify/child.rs` `check`/`run_acceptance`) applies the selection through `with_only`.
   - A Complete always runs every criterion and never carries a selection.
   - Tests in `tests_submit.rs`:
     - a subset request joins a running full Check;
     - a full request does not join a subset job;
     - two equal subsets join.
4. **Reuse pin.**
   - In `host_verify/tests_child.rs`, beside `a_no_cache_job_runs_a_criterion_the_cache_would_answer` (:361), add `a_complete_after_a_passing_check_reuses_every_cached_pass`: run a Check job, then a Complete job on the same head and tree. Every criterion of the second reports cached, in the same way the existing test reads "(cached)", and a spawn observer counts zero criterion processes.
   - If it fails, find the field that differs between the two children's `CriterionContract` or `InputFingerprint` (`verify/criteria/cache_contract.rs`, `cache_fingerprint.rs`) and make the two kinds compute the same inputs. Never weaken certification.
   - If the Complete kind's child path now lives in a file outside your table (daemon-completion may have moved it), record that with `loom memory note` and report it. Do not edit the file.

## Contracts you must turn green

`check-criterion-runs-only-named-criteria` and `full-check-never-joins-a-subset-job` in `tests/gate_dedup_contracts.rs`.

## Proof

`cargo test --lib host_verify::`, `cargo test --lib verify::criteria::`, `cargo test --test gate_dedup_contracts check_criterion full_check`.
