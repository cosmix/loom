# dispute-kinds / W3 — adjudication signal and prompt dispatch, failure messages, feedback

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D15, D16. Knowledge:
`architecture/adjudication-lifecycle.md` "What Each Verdict Delivers";
`mistakes/adjudication-autonomy-deadlock.md` "A Prompt Named a Rust Type as Its Schema and Every
Judge Guessed the Same Wrong Shape". Code: `orchestrator/signals/adjudication.rs` (the
adjudication signal; read it in full first), `orchestrator/adjudication/prompt.rs` (347;
`Prompt { instructions, evidence }`, `MAX_PROMPT_BYTES`, `build_instructions` L80-115,
`build_evidence`), `prompt/{execution_site,sources,truncate}.rs`, `signals/generate.rs::append_stage_feedback`
(L311-335).

Pinned from W1: `DisputeKind`, `FindingSnapshot`, `IntegritySnapshot`.

## Files you own

`loom/src/orchestrator/signals/adjudication.rs`, `loom/src/orchestrator/adjudication/prompt.rs`,
`loom/src/orchestrator/signals/v2_section_review.rs`, `loom/src/orchestrator/signals/generate.rs`,
`loom/src/verify/contracts/completion.rs`, `loom/src/verify/review/gate.rs`,
`loom/src/verify/integrity/gate.rs`.

## Interface you publish for the prompt units (pinned)

```rust
// orchestrator/adjudication/prompt.rs
pub(crate) struct KindPromptInput<'a> {
    pub stage: &'a Stage,
    pub dispute_id: u32,
    pub request: &'a DisputeRequest,
    pub site: &'a ExecutionSite,           // existing type from prompt/execution_site.rs
    pub worktree: Option<&'a Path>,
    pub work_dir: &'a Path,
}
// Each prompt unit exposes:  pub(super) fn build(input: &KindPromptInput<'_>) -> Prompt
```

Declare `mod findings; mod contract; mod integrity;` in `prompt.rs`. Route by
`request.kind`: `Criterion` stays on today's builder; the three new kinds go to their module's
`build`. Instructions are never truncated, and the 100,000-byte cap applies as today (reuse
`truncate.rs`).

## Tasks

1. `prompt.rs` and `signals/adjudication.rs`: route by kind as above. The judge's JSON contract
   per kind is written out in full inside each unit's instructions (no Rust type names as the
   schema; see the knowledge heading above).
2. Failure messages now name the dispute commands:
   - contract completion: "restore it with `loom stage contracts restore <stage>` or dispute it
     with `loom stage dispute-contract <stage> --contract <id> --reason ...`";
   - review gate: "... or dispute them together with `loom stage dispute-findings <stage> --finding <id> ... --reason ...`";
   - integrity gate: "... or dispute it with `loom stage dispute-integrity <stage> --event <id> ... --reason ...`".
3. `v2_section_review.rs`: the review gate block gains the dispute commands, and "file every
   dispute from one review round in one command".
4. `generate.rs::append_stage_feedback`: render adjudicator feedback when any dispute counter
   (criterion, finding, contract, integrity) is non-zero, not only `dispute_count`.

## Tests

Unit tests for the routing (each kind reaches its builder; criterion output unchanged byte for
byte against the pre-change builder on the same input), and for each failure message naming its
command. No named test is counted for you.

## Proof (one command, once, after the prompt units return)

`cargo test --manifest-path loom/Cargo.toml --lib orchestrator::adjudication::prompt`

## Report

Files changed; the `KindPromptInput` definition as written; the proof result.
