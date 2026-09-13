# Verification ownership and preserved optimizations

## Boundary

Terra/xhigh worker; read this entire brief. No git, verification commands, new subagents, or knowledge edits. The Opus/high stage orchestrator owns verification. Own `loom/src/orchestrator/signals/cache.rs`, `tests_cache.rs`, `tests_doctrine.rs`, `tests_doctrine_prefixes.rs`, `tests_commit_timing.rs`, `tests_size.rs`, and new `loom/tests/token_optimization_contracts.rs`. Other signal, usage, knowledge, cache implementation, hook, model and agent files are read-only. If an asserted invariant fails because another stage's implementation is incomplete, return the exact failure to its owner; do not silently weaken the assertion.

## Source and consumer

`cache.rs::generate_integration_verify_stable_prefix` currently tells every build/test/sandbox or functional verifier to run the complete suite. Its inline tests require the blanket override. `CODE_STAGE_REVIEW` separately requires the gate green again after review fixes. Preserve the latter and all six independent-review dimensions. Existing cache correctness was repaired in measurement-and-cache; do not add another cache or assume command-string equality proves reusable evidence.

## Change

Name one canonical verification owner per immutable tree/environment/criterion contract. IV orchestrator assigns that owner explicitly; other reviewers inspect independently and may request a targeted discriminating check, but do not each run the entire canonical suite by default. A targeted security/functional check is additional evidence, never a substitute for the full integration gate. Do not reduce review dimensions, code inspected, warning policy, required commands, or final post-fix verification.

Update generated doctrine and its tests together. Preserve complete failure logs as existing evidence artifacts; summaries include command, real exit, evaluated criterion verdict, input/contract identity, actual elapsed time and evidence pointer when available. Do not invent unavailable hashes in a prompt. A missing receipt means run the check, not assume success. After any review fix, the owner re-evaluates invalidated checks; the repaired criteria cache alone decides whether a fully evaluated pass remains reusable. Never permit stale cache evidence to waive a failing gate.

Add the new public-API integration test file as a cross-stage regression matrix. Use existing temporary-repository/test-command fixtures, not the operator's real home or shared state. Tests must call real exported behavior; private signal expectations remain in owned unit tests. Include neighboring mutations that make each test fail under a plausible wrong implementation.

## Preserve and prove

- P1/P10: source-defined 800k defaults and real 80%/100% feedback are not lowered; no forced 40-request reset or maxTurns reduction. Pin current configured defaults through public model construction and signal tests, not source-text greps alone.
- P2/P5: maintain current template/prefix/overview budgets and `plan_overview: false`; required ownership/completion information remains present. Reuse existing signal-size tests; do not lower their caps.
- P6/P9: explicit file briefs, independent review, one-shot ordinary workers and one healthy background wait remain compatible. A completion notification or forwarding acknowledgement is not proof that a Codex job finished.
- Cache: public acceptance execution fails both cold and warm for forbidden stdout; positive stdout and empty-stderr contracts stay truthful; changed context invalidates a pass. Reuse new fully evaluated cache contract, never the old diagnostic's assertion that a false pass is expected.
- Accounting: wrapper and execution counters stay provider-separated; absent quota/usage data never becomes zero savings. CLI comparison behavior is tested by the comparison owner, not duplicated here.

## Orchestrator proof

Run signal unit tests and `cargo test --offline --locked --manifest-path loom/Cargo.toml --test token_optimization_contracts`, plus stage build/clippy/fmt. The new test target must be selected by the IV `--all-targets` gate. No CI workflow edit is necessary. Full canonical tests remain in IV, after all implementation and review fixes.
