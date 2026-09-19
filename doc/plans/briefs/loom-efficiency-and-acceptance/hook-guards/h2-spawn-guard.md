# H2 — spawn guard prepends the subagent preamble

Tier: opus (`loom-senior-software-engineer`). Read `../common.md` first, in particular the
"Subagent preamble file" contract.

## Goal

The orchestrator stops typing the 673-token subagent preamble into every spawn prompt; the spawn
guard adds it. Evidence: report section 4.2 (2d): 1,030 spawns, the preamble is 41% of the mean
brief and is written as output tokens at the main session's rate. This must work in every
session, inside or outside a loom stage.

## Files you own (write)

- `loom-hooks/spawn-guard.sh`
- `loom-hooks/_subagent-preamble.txt` (new)
- `loom/src/fs/permissions/constants.rs` and `loom/src/fs/permissions/tests/hooks_tests.rs`
- `loom/tests/integration/hooks_spawn_guard.rs`, `hooks_spawn_guard_gate.rs`,
  `binary_spawn_guard.rs`

Read-only: `CLAUDE.md.template` Rule 5 (source of the preamble text; the doctrine stage edits the
template after you), `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` (BLOCK-A, BLOCK-D),
`doc/loom/knowledge/entry-points/hooks.md:106-115` (what registering a hook file costs).

## Where things are

`spawn-guard.sh` (392 lines): enforcement gate 92-95 (deny only when `LOOM_STAGE_ID` is set and
`LOOM_MAIN_AGENT_PID` is a live ancestor; otherwise warn); untyped-spawn handling 216-235; model
fill-in through `updatedInput.model` 245-265 using `resolve_defined_tier` (155-195); escalation
warning 255-264; preamble detection 272-277 by substring match of `PREAMBLE_LINE` (73),
skipping `loom-codex-forwarder`; the only prompt rewrite today appends the worker brief
(310-313). Siblings are found with `$(dirname "$0")`.

## Steps

1. Create `_subagent-preamble.txt` with the Rule 5 fenced block's text, unchanged, first line
   equal to `PREAMBLE_LINE`. Register it in `constants.rs` beside the other sourced files
   (`include_str!` constant plus `LOOM_HOOKS` entry) and update the count assertions in
   `hooks_tests.rs`. It has no trigger row: it is data, not a hook.
2. In `spawn-guard.sh`, when the prompt lacks `PREAMBLE_LINE` and the agent type is not
   `loom-codex-forwarder`, set `updatedInput.prompt` to preamble, blank line, original prompt,
   then the worker brief as today. When the prompt already has the line, leave it alone. Replace
   the current missing-preamble warning with this behaviour. If the file is unreadable, keep
   today's warning and change nothing.
3. Coordinator and worker preambles stay orchestrator-written; do not inject them.
4. Wording fix: the untyped-spawn message says "Pass `model` only to escalate" even when the
   spawn set `model`. When `MODEL_REQ` is non-empty, say instead that a generic agent type was
   used with an explicit model and name the typed agents.
5. Once per session (ledger kind `spawns`, same helper pattern as the read ledger), on the first
   spawn, add one advisory line: load the `loom-orchestration` skill before delegating if it is
   not loaded. The skill is created by the doctrine stage; the line is plain text.

## Traps

- One JSON object on stdout. Build `updatedInput` once, carrying model and prompt together; two
  objects or two `updatedInput` keys lose one of the edits.
- `doc/loom/knowledge/mistakes/session-identity-env.md`: the `LOOM_*` exports are a contract;
  outside a stage they are absent and every code path must still work.
- A prompt can be large. Pass it through `jq --arg`/`--rawfile`, never through shell
  interpolation.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --test integration hooks_spawn_guard`
— run once. New cases: prompt without the line gains it exactly once and keeps its body
byte-for-byte; prompt with the line is unchanged; forwarder untouched; works with no `LOOM_*`
environment; model fill-in and prepend arrive in the same `updatedInput`.
