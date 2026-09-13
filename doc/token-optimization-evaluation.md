# Token optimization evaluation

`loom usage --compare <artifact.json>` evaluates a version-1 paired experiment offline. It does not discover transcripts, poll a provider, change a model, alter cache policy, cancel a stage, or perform rollback. Add `--json` for the machine-readable result.

Comparison mode rejects explicit transcript-selection, provider, root, project, stage, plan, and window flags. The ordinary `--since`, `--provider`, and `--windows` defaults do not count as explicit; `--json` remains allowed.

The evaluator makes a bounded claim about the supplied finite fixture set. Exit 0 means every pair has supported subscription, quality, and latency evidence. Exit 1 means at least one candidate was rejected. Exit 2 means the evidence is inconclusive. A malformed or larger-than-16-MiB artifact also exits 2 and emits a reason code in JSON mode.

## Artifact contract

The strict JSON schema starts with `schema_version: 1`, an `intervention` manifest, and `pairs`. The manifest records its identifier, baseline and candidate implementation revisions, and the explicitly changed dimensions (`implementation-revision`, `provider-assignment`, `model-assignment`, or `effort-assignment`). Unlisted workload, input, environment, contract, model, or effort differences make a pair incomparable.

Each pair contains an opaque `work_unit_id` plus `baseline` and `candidate` runs. A run records workload and fixture IDs, source-input revision and digest, environment and acceptance-contract IDs, implementation revision, provider/model/effort assignments, acceptance and semantic-completion evidence, required checks, independent review dimensions, unresolved findings, omissions, retry/fix counts, both latency measures, accounting provenance, provider-native token vectors, and optional quota intervals. Identifiers are bounded and cannot contain path separators. Unknown fields are rejected, which keeps prompts, output, paths, secrets, prices, and account identity outside this artifact.

## Two independent verdicts

The token-proxy verdict compares raw normalized dimensions within each provider. Claude and Codex vectors remain separate. Composite totals are not added to their component dimensions, Codex reasoning is not added to output, and Codex tokens are never translated through Claude S1/S2/S3 accounting. A proxy improvement requires no increase in any comparable dimension and a strict decrease in at least one. A provider transfer, cache-category tradeoff, missing dimension, or unchanged vector is inconclusive as an improvement claim.

The subscription verdict uses quota observations rather than token weights or public API prices. Supporting evidence requires matching provider/window sets and reset identities, same-reset continuity, known precision, no unrelated concurrent usage, no worse observed consumption in any interval, and a reduction larger than combined measurement uncertainty in at least one interval. Reset crossings, missing windows, coarse unchanged values, decreases within one reset interval, and unknown concurrent use are inconclusive.

A supported token proxy never becomes a supported subscription verdict by itself. The overall verdict requires subscription evidence plus complete quality and latency evidence.

## Paired canary protocol

Live trials consume subscriptions and require explicit scheduling and user authority. Before a trial:

1. Freeze the workload fixtures, source-input revision and digest, environment identity, acceptance contract, required commands, review dimensions, providers, models, and effort. Record baseline and candidate implementation revisions and name the one intervention.
2. Include successful work, expected failures, recovery paths, and review-heavy work. Preserve the same required checks and independent review dimensions for both arms.
3. Predeclare cold-start and warm-start strata. Do not combine them after observing results.
4. Interleave baseline and candidate order across repetitions to reduce time-order bias. Retain every outcome, including failures, retries, fixes, and incomplete runs.
5. Record wall-clock critical path and notification latency for each run. Missing measurements are unknown, not zero.
6. Record provider-native normalized token vectors and accounting provenance. Keep raw prompts, model output, filesystem paths, secrets, prices, and account identity out of artifacts.
7. Capture quota interval endpoints for every relevant provider/window with observation times, reset identity and time, precision, continuity, and an explicit unrelated-concurrent-use assessment.
8. Repeat each stratum until the observed uncertainty is characterized well enough to distinguish a reduction from quota precision. A fixed repetition count is not evidence by itself.

### Collecting run evidence

- **Token vectors.** Run `loom usage --provider all --since <start> --until <end> --json` over each run's window, narrowed with `--stage`/`--plan`, or with explicit `--claude-root`, `--codex-root` and `--forward-receipts-root` when the run lives outside the default project. Only `measured-canonical` rows are measurements. Carry `duplicate-exact`, `ambiguous-conflict`, `fallback-coverage`, `synthetic`, `unknown-usage` rows and every absent dimension into the artifact as missing, never as zero.
- **Codex worker cost.** `--forward-receipts-root <project>/.loom/work` joins forwarder transcripts to Codex threads through forward receipts. A forward without a receipt stays unattributed.
- **Quota intervals.** The ledger's `quota_history` section, when present, lists observations whose continuity is `initial`, `same-reset`, `reset` or `unknown`. An interval that is not `same-reset` from end to end cannot support a subscription reduction.
- **Binary.** The `loom` that evaluates the artifact must contain the comparison command. Check that `loom usage --help` lists `--compare` before the trial starts.

### Assumptions a canary must sample before claiming them

- Read-receipt reuse assumes Claude Code writes a `Read` tool_use and its tool_result as adjacent transcript rows, and writes the result before PostToolUse hooks run. Neither is measured. Sample real transcripts with serial and parallel `Read` calls and confirm receipts form before attributing any repeat-read saving.
- Stage ledgers written before 2026-09-13 (`.loom/work/subagents/<stage>/codex.jsonl`) contain fake forward records that hook tests appended; never use them as provenance.

Run each retained artifact through the real command:

```text
loom usage --compare experiment.json --json
```

Do not average away a regression. One new defect, omitted requirement, weakened check or review, failed semantic completion, slower critical path, or slower notification rejects the candidate even when other pairs improve enough to lower an average.

## Decision and rollback

Candidate policies remain conditional until the paired canary has complete subscription, quality, and latency evidence. Token-proxy improvement alone can guide further measurement but cannot justify a subscription-savings claim.

If any canary shows a quality or latency regression, reject the candidate and use the operator's ordinary scoped deployment revert procedure. The evaluator does not automatically kill workers, cancel stages, switch models, mutate cache settings, or roll back a deployment. Preserve the rejected evidence for the next investigation.
