# Provider-separated Pareto evaluation

## Boundary and source

Sol/xhigh worker. No git, verification commands, subagents, live model requests or knowledge edits. Own `loom/src/commands/usage/**` and new `loom/tests/token_optimization_comparison.rs`; read all previous usage changes directly. The Terra verification worker owns the other token-optimization integration target. `UsageArgs` and `execute` in `commands/usage/mod.rs` are the actual CLI entry: extend that existing path, not a second report executable.

## Public contract

Add optional `--compare <artifact.json>` to `loom usage`; it is an offline mode mutually exclusive with transcript selection/root/project/stage/plan flags. It does not discover transcripts or poll a provider. The referenced `--provider`/`--claude-root`/`--codex-root`/`--receipts-root` flags do not exist in `loom/src/commands/usage/mod.rs` at `7d6a14ca`; they arrive with measurement-and-cache, which this stage depends on. `--json` emits the comparison result; without it emit a compact human explanation. Add new private `comparison.rs` and focused tests, splitting helpers below 400 lines/50 lines. Use serde's strict schema validation and bounded input size (16 MiB, design limit, never a token-saving claim).

Expose the private `comparison::compare` entry and call it from `mod.rs::execute` before discovery when the explicit comparison flag is present. Clap defaults are not explicit selection flags: ensure default `--provider claude` and default `--since` do not falsely conflict with comparison mode; use argument-source detection or an equivalent tested explicit-mode validator.

The version-1 input is `{schema_version: 1, pairs: [...]}`. Each pair has an opaque `work_unit_id`, `baseline` and `candidate`. Each run declares:

- workload/fixture identity, relevant source-input revision/digest, environment identity, acceptance-contract digest, provider/model/effort assignments, and run id;
- accepted status, required checks with command/contract identity and verdict, independently reviewed dimensions, unresolved findings by severity, omitted requirements, retry/fix counts, and semantic completion outcome;
- measured wall-clock critical-path milliseconds, notification latency milliseconds, accounting completeness/provenance, and raw provider-specific normalized token vectors from the previous stage's ledger;
- optional quota observations per provider and window, including reset identity, observation times, utilization precision, unrelated concurrent-use indicator and continuity. Absence is unknown.

Validate identifiers and limits without persisting raw prompts, output, paths, secrets, prices or account identity. No floating negative tokens, reasoning added to output, or cached input added twice. Share provider normalizer definitions; do not translate Codex tokens through Claude S1/S2/S3. Workload/input/environment/contract/model differences that are not the explicitly declared intervention make a pair incomparable. The experiment manifest must identify that intervention; a code optimization naturally changes candidate revision, but the tested workload and required contract must remain equivalent and both revisions must be recorded.

## Decision semantics

Return per-pair and aggregate `rejected`, `inconclusive`, or `supported-candidate`, with explicit reason codes and both provider vectors. Any missing requirement, new unresolved defect, unsuccessful semantic completion, weakened verification/review, or observed increase in critical-path/notification latency rejects the candidate. Never average away a quality or latency regression across other pairs. Missing quality/latency evidence is inconclusive, never a pass. Finite fixtures support a bounded claim, not a universal zero-regression guarantee.

For token evidence, report raw dimensions and uncertainty, not a single weighted subscription score. A token-proxy improvement requires no increase in any comparable provider token dimension and a strict reduction in at least one; classify it as a proxy only. Cache-category tradeoffs or movement between providers are incomparable without measured subscription evidence. Do not assume cached tokens are free or public API prices describe subscription debits.

Subscription-supported improvement additionally requires comparable quota intervals with identical reset boundaries, known precision, no unrelated concurrent usage, and no worse consumption for any provider/window. The reduction must exceed measurement uncertainty. Reset crossings, coarse unchanged percentages, missing windows and same-reset decreases yield inconclusive subscription attribution. Both baseline and candidate zero-delta coarse observations cannot establish savings. The overall output separates token-proxy verdict from subscription verdict; never promote the former into the latter.

Exit 0 only for fully supported subscription/quality/latency evidence; exit 1 for rejected; exit 2 for inconclusive. Always emit valid JSON in JSON mode, including reason codes for malformed input where possible. These are comparison outcomes, not code correctness verdicts. No automatic rollback, stage cancellation, model switch or cache policy mutation.

## Rollout evidence

Provide a sanitized fixture matrix in `loom/tests/fixtures/token_optimization/comparison/` (also owned): valid improvement, cross-provider transfer, output increase/input decrease, reset crossing, coarse quota, missing telemetry, model/effort mismatch, failed review, omitted requirement, slower critical path, slower notification, incomparable workload, duplicate run id, malformed/oversized schema, and a candidate that appears good only when a regression is averaged away. Tests construct or read fixtures and execute the real CLI binary through the existing integration process harness; unit tests alone do not prove dispatch.

Do not run subscription-consuming experiments automatically. Document in `doc/token-optimization-evaluation.md` (owned) a paired canary protocol: freeze task/required checks and models, interleave baseline/candidate order, include success/failure/recovery/review-heavy cases, record cold/warm starts, repeat until uncertainty is characterized, retain all outcomes, and roll back an observed quality/latency regression through the operator's ordinary deployment procedure. Live trials need explicit scheduling and user authority. Candidate policies remain conditional until this evidence exists.

## Orchestrator proof

Run the usage library tests and `cargo test --offline --locked --manifest-path loom/Cargo.toml --test token_optimization_comparison`, with stage build/clippy/fmt. Assert process exits 0/1/2 and exact decoded output fields, not substring matches. The new integration target is selected automatically by IV `--all-targets`.
