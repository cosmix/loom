# review-harvest-gate / W1 — review store and gate

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D12. Knowledge:
`conventions/dispute-and-adjudication.md` "Dispute File Ownership Convention" (the party under
review must never be able to write its own verdict); `patterns/stage-lifecycle-and-verification.md`
"Degraded Modes Are Reported, Never Silent".

The codex units write `verify/review/report.rs` and `verify/review/fingerprint.rs`. Their APIs are
pinned in `cx-parser-fingerprint.md`; read it. W2 calls your store API from the harvest
delegate and the status command.

## Files you own

`loom/src/verify/mod.rs` (one line), `loom/src/verify/review/mod.rs` (new; declares `report`,
`fingerprint`, `store`, `gate`), `loom/src/verify/review/store.rs` (new),
`loom/src/verify/review/gate.rs` (new), `loom/src/verify/review/gate_tests.rs` (new),
`loom/src/commands/stage/complete_verification.rs`.

## Store API you publish (pinned for W2)

```rust
pub struct ReviewRound { /* DESIGN D12 round-<n>.json: version, round, agent_id, harvested_at, fingerprint, files, malformed, findings (with ids), resolved, unresolved, suggestion_memory_ids */ }
pub struct RecordedFinding { pub id: String, #[serde(flatten)] pub finding: report::Finding }
pub fn next_round(work_dir: &Path, stage_id: &str) -> Result<u32>;
pub fn write_round(work_dir: &Path, stage_id: &str, round: &ReviewRound) -> Result<()>;  // create-new, never overwrite
pub fn load_rounds(work_dir: &Path, stage_id: &str) -> Result<Vec<ReviewRound>>;         // sorted by round
pub fn load_rulings(work_dir: &Path, stage_id: &str) -> Result<Rulings>;                 // absent file = empty
pub fn load_carried(work_dir: &Path, stage_id: &str) -> Result<Carried>;                 // absent file = empty
pub struct OpenFinding { pub id: String, pub origin_stage: Option<String>, pub finding: report::Finding }
pub fn open_findings(work_dir: &Path, stage_id: &str) -> Result<Vec<OpenFinding>>;
```

`Rulings` and `Carried` are the D12 JSON shapes. dispute-kinds writes them later; you only read
them. Paths: `.loom/work/reviews/<stage>/`. Validate `stage_id` as the daemon does
(`validate_id`). Writes are atomic.

## Tasks

1. `store.rs` as above. Finding ids are `F-<round>-<k>` (k from 1). A carried finding's id is
   `<origin-stage>/F-<round>-<k>`. `open_findings` = every finding of every round and every
   carried finding, minus those resolved by a LATER round's `resolved` and those ruled
   `dismiss`/`defer`.
2. `gate.rs`: `pub fn check(stage: &Stage, work_dir: &Path, worktree_root: &Path, target_branch: &str) -> Result<()>`
   for v2 `standard` and `integration-verify` stages. It checks:
   - (a) at least one well-formed round;
   - (b) the latest well-formed round's fingerprint equals `fingerprint::compute(worktree_root, target_branch)`;
   - (c) `open_findings` is empty.

   On failure it bails with one message listing every problem. That message names
   `loom stage review status <stage>` and, for open findings, "fix them and run a re-review, or
   dispute them" (dispute-kinds adds the command name). When the latest round is malformed,
   its reason is quoted.
3. Hook it into `complete_verification::run` (L36-49) after the contract check that
   contract-phase added: one call, for `plan_version == 2` and standard or IV stages, using the
   `base_branch` that `run` already computes (L39) as `target_branch`.

## Named tests (binding), in `gate_tests.rs`

- `gate_fails_without_review_at_current_fingerprint`: one round recorded at fingerprint X; the
  worktree now differs → error naming the review status command.
- `gate_fails_with_open_finding`: latest round matches and has one finding → error listing its id.
- `gate_passes_when_later_round_resolves_finding`: round 1 has F-1-1, round 2 (current
  fingerprint) lists `resolved: ["F-1-1"]` → Ok.

## Proof (one command, once, after the codex units return)

`cargo test --manifest-path loom/Cargo.toml --lib verify::review::`

## Report

Files changed; `complete_verification.rs` line count (must stay ≤ 400); the proof result.
