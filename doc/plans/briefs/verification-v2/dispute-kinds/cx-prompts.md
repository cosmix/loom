# dispute-kinds / codex units — judge prompts per kind

For the orchestrator: three `loom-codex-forwarder` units, in the foreground,
`--model gpt-6-sol --effort xhigh`, an explicit 600000 ms Bash timeout, spawned after W1 and W3
report. Paste the shared block and the unit's block verbatim, plus W3's reported
`KindPromptInput` definition and W1's `DisputeKind`/`FindingSnapshot`/`IntegritySnapshot`
definitions. Tell every unit NOT to run git and NOT to touch `.loom/`; check
`git status --short` after each. W3 has declared the modules, so each unit can compile; its proof
is `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::adjudication::prompt::<module>`.

## Shared

- Design: `doc/plans/briefs/verification-v2/DESIGN.md` D15.
- Start from `loom/src/orchestrator/adjudication/prompt.rs` (`build_instructions`,
  `build_evidence`, `Prompt`) and mirror its structure. The instructions keep the existing parts
  that are not criterion-specific (the job, recording the verdict with `loom stage adjudicate`,
  where the draft goes) and replace "RUN THE CRITERION" with the kind's first step below.
- Spell out the verdict JSON in the instructions with a literal example, never a Rust type name.
- Evidence sections degrade to a message in place when a source is missing, as `build_evidence`
  does. Never panic.
- File ≤ 400 lines, functions ≤ 50, tests at the end of the file: one test that every evidence
  section appears for a full input, and one that a missing worktree degrades.

## CX-1 — `prompt/findings.rs`

First step: "Read each finding's cited code and decide, for each, whether the scenario or cited
rule holds against the current code. Run a narrow check when one settles it." Evidence, per
finding in `DisputeKind::Findings.evidence`: id, severity, `file:line`, claim, scenario or rule,
review round; the cited file ±20 lines around `line`, read from the worktree; `git diff` of that
file against the stage base. Then the agent's reason. Verdict JSON:

```json
{"verdict":"rulings","rulings":[{"finding":"F-1-2","ruling":"uphold","target_stage":null,"reasoning":"…","citations":[{"file":"src/a.rs","line":42,"excerpt":"…","claim":"…"}]}]}
```

`ruling` is `uphold`, `dismiss` or `defer`. `defer` needs `target_stage`: a later stage of this
plan that depends on this one. An integration-verify stage never defers. Or
`{"verdict":"needs-more-evidence","questions":["…"]}`.

## CX-2 — `prompt/contract.rs`

First step: "Run the contract test as the adapter runs it, then compare the frozen content with
the current content." Evidence:

- the contract spec (`id`, `file`, `test`, `scenario`, `rejects`) from the stage;
- the frozen file (`verify::contracts::store::frozen_file_path`);
- the current file from the worktree;
- a unified diff of the two;
- the agent's reason.

Verdict: `accept` (the change keeps the contract's scenario and its `rejects` clause; optional
`plan_patch` in the existing `{"op":"replace","index":N,"value":{…}}` shape targeting `contracts`),
`reject` (the change weakens the contract), or `needs-more-evidence`; `citations` required for
accept and reject.

## CX-3 — `prompt/integrity.rs`

First step: "Decide whether each change removes protection the tests gave." Evidence per event
in `DisputeKind::Integrity.evidence`: id, kind, language or path, base and current counts, the
`detail` lines (removed or changed assertions), and for a ratchet event the file's diff against
the stage base. Then the agent's reason. Verdict: `accept`, `reject` or `needs-more-evidence`;
`citations` required for accept and reject.
