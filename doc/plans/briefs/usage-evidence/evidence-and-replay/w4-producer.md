# W4: the `RunEvidence` producer

Tier: sonnet (`loom-software-engineer`). Read `common.md` in this directory first.

## Why

`loom usage --compare` consumes `RunEvidence` (`comparison_schema.rs:46-72`, `deny_unknown_fields`), and nothing in the tree writes it (`models/execution_receipt.rs` has consumers only). The closest block, `loom usage --json`, prints a `ProviderLedger` that `--compare` cannot read, and it lacks the run identities an artifact needs. A `loom usage` output mode fills the measured fields; a small declaration file supplies the identities the tooling cannot measure.

## Files you own

- `src/commands/usage/run_evidence.rs` (new), `src/commands/usage/run_evidence_stages.rs` (new), `src/commands/usage/run_evidence_tests.rs` (new, declared from `run_evidence.rs`)
- `src/commands/usage/args.rs`, `src/commands/usage/mod.rs`
- `../README.md` (the `loom usage` lines under `### Other Commands`, one paragraph after that code block, and the regression sentence in the comparison description)

Read-only: `src/commands/usage/comparison_schema.rs` (W1 adds `Serialize`), `src/commands/usage/provider_types.rs`, `src/commands/usage/completeness.rs`, `src/verify/transitions/persistence.rs` (`list_all_stages`), `src/verify/review/store.rs` (`open_findings`, `OpenFinding`), `src/verify/review/report.rs` (severities `critical|major|minor|unspecified`, `:166`), `src/models/stage/types.rs` (`retry_count` `:372`, `fix_attempts` `:465`, `started_at` `:348`, `completed_at` `:344`, `dependencies`, `status`).

## Step 1: the flag (`args.rs`)

```rust
/// Write one run's RunEvidence (the input `--compare` pairs) from a declaration file and this project's measured window
#[arg(long, value_name = "DECLARATION.json",
      conflicts_with_all = ["compare", "all", "provider", "stage", "plan"],
      requires_all = ["project", "since", "until"])]
pub run_evidence: Option<PathBuf>,
```

`has_explicit_selection` stays as it is; clap refuses `--compare` with `--run-evidence`.

## Step 2: wiring (`mod.rs`)

- Declare `mod comparison_latency; mod comparison_net; mod run_evidence; mod run_evidence_stages;` (`common.md` section 2).
- `execute`: right after the `--compare` branch, `if let Some(declaration) = args.run_evidence.as_deref() { return run_evidence::produce(&args, declaration); }`.
- `normalize_usage` takes the provider selection as a parameter instead of reading `args.provider()`, so the producer can normalize both providers; `execute` passes `args.provider()`. Keep every function under 50 lines.

## Step 3: `run_evidence.rs`

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunDeclaration {
    run_id: String, workload_id: String, fixture_id: String,
    source_input_revision: String, source_input_digest: String,
    environment_id: String, acceptance_contract_digest: String, implementation_revision: String,
    assignments: Vec<ProviderAssignment>,
    accepted: Option<bool>,
    required_checks: Vec<CheckEvidence>,
    reviewed_dimensions: Vec<ReviewEvidence>,
    omitted_requirements: Vec<String>,
    semantic_completion: EvidenceVerdict,
    quota_observations: Option<Vec<QuotaObservation>>,
}
pub(super) fn produce(args: &UsageArgs, declaration: &Path) -> Result<()>
```

- Read the declaration bounded at 1 MiB; a parse failure names the file and the serde position, never its content.
- Time range from `args.since` and `args.until`: reject a value that does not parse with `chrono::DateTime::parse_from_rfc3339`, naming the flag, before building the range with `TimeRange::parse`. clap's `requires_all` makes the flags present, not RFC3339, and a relative window cannot be reproduced. Work dir from `usage_work_dir(args.project, false)`; no work dir is an error ("the project has no .loom/work").
- Normalize with `ProviderSelection::All` through `normalize_usage`. For each provider named in `assignments`: sum each of the ten `ProviderTokenVector` fields over the ledger rows of that provider whose `provenance.is_measured()`; a field is `Some(sum)` only when every summed row has it, `None` otherwise (unknown is not zero); a provider with no measured row gets no vector.
- `accounting_provenance`: `Unknown` when an assigned provider has no measured row; else `Partial` when an assigned provider report has `completeness.state == Partial`; else `Measured`. `accounting_complete` is `provenance == Measured`. `provider_tokens` holds the vectors of assigned providers that have one.
- `notification_latency_ms: None`: no producer of notification latency exists, and `--compare` reports it unmeasured.
- Print the `RunEvidence` with `serde_json::to_writer_pretty` to stdout and a trailing newline.

## Step 4: `run_evidence_stages.rs`

```rust
pub(super) struct StageEvidence { pub(super) retry_count: u32, pub(super) fix_count: u32, pub(super) critical_path_ms: Option<u64>, pub(super) unresolved_findings: Vec<FindingEvidence> }
pub(super) fn stage_evidence(work_dir: &Path) -> Result<StageEvidence>
```

- Stages: `crate::verify::transitions::list_all_stages(work_dir)`.
- `retry_count` = sum of `Stage.retry_count`; `fix_count` = sum of `Stage.fix_attempts` (saturating, capped at `u32::MAX`).
- `critical_path_ms`: the longest dependency chain of stage durations, duration = `completed_at - started_at` in milliseconds; cp(s) = duration(s) + max cp over `s.dependencies`. `None` when there are no stage records, any stage is not `StageStatus::Completed`, lacks either timestamp, has a negative duration, or names a dependency that is not among the stages. Waiting between stages is not counted.
- `unresolved_findings`: for each stage, `crate::verify::review::store::open_findings(work_dir, &stage.id)`; a stage without a `reviews/<stage>/` directory yields none (the store already returns empty); any other error aborts with the stage id. `finding_id` = `format!("{}:{}", stage.id, finding.id).replace('/', ":")` (a carried finding id `<origin>/F-r-k` has a `/`, which `valid_id` refuses). Severity mapping, stated in the README: `critical` to `critical`, `major` to `high`, `minor` to `medium`, anything else to `low`. These ids are local to one run: the independent reviewer of a paired trial replaces them with cross-arm ids before `--compare`.

## Step 5: README

Under `### Other Commands` in `../README.md`, change `--since <duration|date>` to `--since <duration|date|rfc3339>`, and add after the `--compare` line:

```text
loom usage --run-evidence <declaration.json> --project <dir> --since <rfc3339> --until <rfc3339> [--claude-root <dir>] [--codex-root <dir>]
                                                                             # Print one run's RunEvidence for --compare
```

After the code block, add one paragraph: `--project` also reads the transcripts of the project's removed stage worktrees; each provider reads `measured` or `partial` with reasons (a session record naming an unread transcript, unknown or ambiguous rows, a forward receipt whose thread has no rollout); Codex rows count for a project only through the threads its forward receipts name, so `--all` is the way to see every Codex rollout; `--run-evidence` takes the run identities from the declaration (fields as in the artifact contract, minus the measured ones), measures tokens, retries, fixes, the critical path and open review findings, and maps review severities critical, major, minor and unspecified to critical, high, medium and low; an artifact may declare `"token_rule": "net-total"` and `"latency_tolerance_ms"`. In the same README, the sentence "any quality or latency regression rejects the candidate" becomes "any quality regression, or a latency regression beyond a declared tolerance, rejects the candidate". Anchor by the heading and the `loom usage --compare` line; plan 3 edits other README sections before this plan runs.

## Tests (`run_evidence_tests.rs`)

Unit tests over a `tempfile` work dir with stage records written by `crate::verify::transitions::create_stage`: two completed stages, b depending on a, durations 10 min and 8 min, give 1,080,000 ms; a running stage gives `None`; retries 1 and 0 with fixes 2 and 1 give 1 and 3; a carried finding id `stage-x/F-1-2` maps to `stage-b:stage-x:F-1-2`; severities map as pinned. Run once at the end: `cargo test --lib commands::usage::run_evidence` and report it with each changed file and its line count after `cargo fmt`.
