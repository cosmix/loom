# Attribution and agent-type sidecar

The stage description defines the public surface and the exact formulas. This brief adds the code seams. Frozen contracts in `loom/src/commands/usage/agent_sidecar/tests.rs` and `loom/src/commands/usage/sections/attribution/tests.rs` must pass. Never edit them.

## Agent-type sidecar

- Create `loom/src/commands/usage/agent_sidecar/mod.rs`. Claude Code writes `agent-<id>.meta.json` beside `agent-<id>.jsonl`. Observed keys include `agentType`.
- Read through `crate::fs::safe_read::read_bounded(root, relative, 64 * 1024)` with the transcript's parent directory as root and the sidecar's file name as relative path. Any failure returns `None`.
- In `parse_all` (`loom/src/commands/usage/mod.rs:68-104`), after the `metadata` block and only when it left `agent_type` as `None`, and only for a subagent file, set `transcript.agent_type`. Do not set `stage_id`, `loom_session_id` or `requested_model` from it.
- `mod.rs` is 392 lines and the file limit is 400. Keep the addition to a `mod` line and a few lines in `parse_all`.
- Consumers that read `agent_type` change behaviour under `--all`: `forward_join.rs:254` and `claude_provider.rs:188`. Trace both and confirm that a sidecar-derived `loom-codex-forwarder` is a correct classification. Report what you find.
- `usage_tests.rs:163-167` and `:212-214` expect `None` when no ledger row exists. Those fixtures have no sidecar, so they should still pass. Confirm rather than assume.

## Attachments

- `transcript.rs` scans rows and `build_transcript` (`transcript.rs:119`) constructs the struct. Add `attachments` to `ScanResult` and `Transcript`.
- Rows have `"type":"attachment"` and an `attachment` object whose `type` names the kind. Observed kinds include `skill_listing`, `instructions`, `prompt_snapshot`, `sandbox_instructions`, `hook_success` and `hook_additional_context`.
- `requests_before` must count distinct requests, because one API response spans several rows sharing `message.id`. Use whatever the scan already uses to dedupe.
- Apply the same `since` cutoff as entries.
- Find every `Transcript {` literal with `rg -n 'Transcript \{' loom/src/commands/usage` (at least `forward_join_tests.rs:90`) and add the field.

## Attribution section

Create `sections/attribution/mod.rs` and register it in `sections/mod.rs`: `pub mod attribution;`, a `Report` field, `attribution::build(transcripts)` and `attribution::render(&report.attribution)`. Render it after `reads`.

Tool-result classes:

| Class | Rule |
| --- | --- |
| `read:skill` | `Read` of a path containing `loom-skill-catalog` or ending `SKILL.md` |
| `read:knowledge` | `Read` under `/doc/loom/knowledge/` |
| `read:plan-or-signal` | `Read` under `/doc/plans/`, `/.loom/` or `/signals/` |
| `read:source-other` | Any other `Read` |
| `bash:build-test` | Bash, after an optional `cd <dir> &&`, starting `cargo`, `bun`, `bunx`, `pytest`, `uv`, `go`, `make`, `tsc` or `eslint` |
| `bash:loom`, `bash:git` | Bash starting `loom ` or `git ` |
| `bash:search` | Bash starting `rg`, `grep`, `fd`, `find` or `ls` |
| `bash:file-read` | Bash starting `sed`, `cat`, `head`, `tail`, `awk`, `less` or `more` |
| `bash:other` | Any other Bash |
| `agent-report` | `Agent` or `Task` |
| `skill-tool` | `Skill` |
| `edit-ack` | `Edit`, `Write` or `MultiEdit` |
| `search-tool` | `Grep` or `Glob` |
| `other-tool` | Anything else |

Other rows:

- `user-text`: a user entry with `tool_use_id` of `None`, at `k >= 1`.
- `attach:<kind>`: an attachment at `k >= 1`.

Join tool results to tool uses through `Request.tool_uses[].id` and `UserEntry.tool_use_id`, as `sections/reads.rs::result_bytes` does. `reads.rs::read_path` is private: make it `pub(super)` and reuse it. Do not write a second path extractor.

`startup_by_agent` groups by `transcript.agent_type`, then `main` for main scope, then `unknown`. It reports the median first-request `usage.resident()` and, for each attachment kind seen at `k == 0`, the median `chars / 4`. Keep the 8 largest kinds per agent type.

Bounds and safety:

- Keep at most 24 attribution rows, largest first, and merge the tail into `other`.
- Attachment kinds and agent types are transcript text: pass them through `inline_safe` when rendering, as `sections/agents.rs` does.
- Label the report as an estimate. Label the replay line an upper bound, and print the residual with its share of the total.
- Print the `Attribution` heading even with no data, then use `no_data`, as other sections do.

Put non-contract tests in `sections/attribution/more_tests.rs` if needed. Each file stays under 400 lines and each function under 50.

## KNOWN_AGENT_TYPES

In `sections/agents.rs:242`, add `"loom-mechanical-engineer"` and change the array length to 7.
