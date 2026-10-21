# Gate Dedup: Common Brief

## 1. Purpose

Loom stages in every project run the same verification several times per tree, and an accepted contract dispute leaves harness files frozen at stale hashes. The plan is `doc/plans/IN_PROGRESS-PLAN-gate-dedup.md` (Overview, Causes table). Every change must work for any project loom drives and for every test runner loom has an adapter for (`loom/src/testrun/adapters/`, 24 runners), not for loom's own suite.

## 2. Facts at `1b5a0c91` (re-read at your stage base; host-verification's daemon-completion stage landed after this)

- Criteria numbering is 0-based everywhere: `commands/stage/acceptance_runner.rs:190`, `commands/stage/criterion_output.rs:34-36` (`FAILED [criterion {index}]`), `loom stage dispute-criteria --criterion-index` (`stage_service/dispute.rs:146`). The `--criterion` flag uses the same base.
- `loom check` is `Commands::Check { stage_id, suggest, no_cache }` (`cli/types.rs:290`), dispatched at `cli/dispatch.rs:360` to `run_check` (:190), which calls `commands/verify.rs:21` `execute(stage_id, suggest)`. `execute` exits non-zero on a failed criterion (`ensure_passed`, :87).
- Host jobs: `host_verify/submit.rs` `submit` (:52-83) and `covers` (:105-110) compare head, tree_digest, no_cache and kind. `SubmitRequest` (:23), `JobRecord` (`host_verify/mod.rs:81`), relay `HostJobRequest { head, tree_digest, no_cache }` (`relay/payload.rs:48`, `deny_unknown_fields`). The child (`host_verify/child.rs` `run_job` :34) runs `run_acceptance_with_config` with the cache in the state work dir unless `no_cache` (:125-129).
- `TestRunnerAdapter` (`testrun/mod.rs:83-130`) has `recognizes(argv)` and `is_full_run(argv)` on every adapter; `testrun/recognize.rs` `strip_prefixes` sees through `bunx`, `npx`, `pnpm exec`, `yarn`, `uv run`, `env`.
- Size rules: Rust files at most 400 lines, functions at most 50. `loom/maintainability-baseline.txt` is a ratchet file. Its rows `function src/commands/verify.rs execute 63` and `function src/cli/dispatch.rs dispatch 69` must stay exact, so keep both functions line-neutral. `CLAUDE.md.template` has 160 bytes of headroom under `CLAUDE_MD_TEMPLATE_MAX_BYTES` (20,480, `orchestrator/signals/tests_size.rs:38`).

## 3. Pinned interface (the contracts depend on it)

- `loom check <stage-id> [--criterion <n>]...`. The flag is repeatable, the indices are 0-based, and an index past the list is an error naming the list length. The summary of a subset run prints `subset: <indices>`.
- `loom::host_verify::submit::SubmitRequest` gains `pub criteria: Option<Vec<usize>>`, where `None` means every criterion. `JobRecord` and the relay `HostJobRequest` carry the same field (`#[serde(default)]` on the record only if old records must load, which they need not: no compatibility code, per the project's CLAUDE.md).
- The re-freeze reads its file set through a new `loom::verify::contracts::frozen_set` module. It is shared by the first freeze (`stage_service/contracts.rs::read_files`) and by accept.

## 4. The new gate flow (W5 writes it; everyone follows it while working)

- Implementers run filtered tests only.
- A standard stage's verifier runs `loom check <stage>`, plus only the steps the plan lists outside acceptance. A fix round runs `loom check <stage> --criterion <n>` for the failed criteria plus the lint and format criteria. `loom stage complete` runs once on the final tree.
- The full suite runs once, in integration-verify.

## 5. Conduct

- Read `doc/loom/knowledge/INDEX.md` only if this brief leaves a question open, and then only the section it names.
- Use `loom map --outline/--find-all/--impact` before opening a file. Read ranges, not whole files.
- Run at most your own filtered tests (for example `cargo test --lib host_verify::`), never the full suite, clippy over the workspace, or `loom check`: the gate runs after you return.
- No git commands. Never edit the frozen contract files `tests/gate_dedup_contracts.rs` and `tests/gate_dedup_contracts/**`.
- Record surprises with `loom memory note`. Return a short report: files changed, the test command you ran and its result, anything unresolved.
