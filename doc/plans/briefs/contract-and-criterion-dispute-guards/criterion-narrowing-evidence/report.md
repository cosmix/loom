# Brief: report (stage criterion-narrowing-evidence)

## Goal

Integration-verify and the operator see every test an amendment left out and every amendment of the plan.

## Files you own

- `loom/src/orchestrator/signals/handoffs.rs` (new), `loom/src/orchestrator/signals/mod.rs`, `v2_section.rs`, `parse.rs`
- `loom/src/daemon/protocol.rs`, `loom/src/daemon/server/status.rs`
- `loom/src/commands/status/render/summary.rs`
- `README.md`, `skills/**`

Frozen contract files are read-only. Report every row of `loom/maintainability-baseline.txt` that must change.

## Types you compile against (the `narrowing` worker writes `verify/review/handoffs.rs`)

```rust
pub struct Handoff { pub stage: String, pub field: String, pub index: usize, pub tests: Vec<String>,
    pub runs_at: String, pub reason: String, pub dispute: String, pub verified: bool }
pub fn load_handoffs(work_dir: &Path, stage_id: &str) -> anyhow::Result<Vec<Handoff>>
```

## Signal section (`signals/handoffs.rs`)

```rust
pub struct AmendmentLine { pub stage: String, pub field: String, pub op: String,
    pub index: usize, pub reason: String }
pub fn append_handoffs(content: &mut String, handoffs: &[Handoff], amendments: &[AmendmentLine])
```

- Nothing is appended when both lists are empty. Otherwise a `## Handed-Off Tests` section: an instruction line, then one line per hand-off (`<stage> <field>[<index>]: <tests, comma separated> -> <runs_at>: <reason>`, with ` (unverified)` when `verified` is false), then `### Amendments` with one line per amendment (`<stage> <field> <op> [<index>]: <reason>`).
- Instruction line: "Each test below was left out of a stage's check by an adjudicator. Your full-suite criterion runs the ones that can run here: a failure among them is yours to fix or dispute. For each one that cannot run here, record where it runs with loom memory."
- Called from `append_v2_section` (`signals/v2_section.rs:~19`) for `StageType::IntegrationVerify` only, beside `append_reviewer_suggestions` (`signals/v2_section_review.rs:134`): collect `load_handoffs` for every stage of the plan, and the amendments from the audit rows (`plan/amendment/journal.rs:76`, `AuditRow`; use its reader, do not parse the markdown table again).
- Add the heading to the known-section list in `signals/parse.rs:~45-60` ("Reviewer Suggestions" is at `:57`), which `validate_signal_format` reads.

## Completion summary

- `CompletionSummary` (`daemon/protocol.rs:42`) gains `handoffs: Vec<String>` and `amendments: usize`, both `#[serde(default)]`; `collect_completion_with_details` (`daemon/server/status.rs:~83`) fills them: one rendered line per hand-off, as in the signal.
- `write_completion_summary` (`commands/status/render/summary.rs:21`) prints, when either is non-zero: "Adjudicators amended <n> checks in this plan." and "Tests left out of a stage check:" followed by the lines. Foreground mode prints it through `commands/run/foreground.rs:~140` unchanged.

## Docs

`README.md` (disputes, `loom check`): `--at-base`, `leaves_out`, the hand-off section and the summary lines. `skills/loom-plan-writer/SKILL.md` Section 9 and `skills/loom-plan-writer/references/v2-contracts.md` Section 5: an adjudicator narrows a criterion only on a failing base run and records what it leaves out; a test the stage itself adds is never narrowed.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::signals::` and `--lib commands::status::` pass, with a test that an integration-verify signal lists a recorded hand-off and a standard stage's signal does not.
- Report every assertion you changed in existing tests.
