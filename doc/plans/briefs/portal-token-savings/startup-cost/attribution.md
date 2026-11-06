# Attribution and agent-type sidecar

The stage description defines the public surface and the exact formulas. This brief adds the code seams. Frozen contracts in `loom/src/commands/usage/agent_sidecar/tests.rs` and `loom/src/commands/usage/sections/attribution/tests.rs` must pass. Never edit them.

## Agent-type sidecar

- Extend the scaffolded `loom/src/commands/usage/agent_sidecar/mod.rs`, keeping its `#[cfg(test)] mod tests;` line. Claude Code writes `agent-<id>.meta.json` beside `agent-<id>.jsonl`. Observed keys include `agentType` and, on team agents, `spawnedAgentType`.
- Read through `crate::fs::safe_read::read_bounded(root, relative, 64 * 1024)` with the transcript's parent directory as root and the sidecar's file name as relative path. Any failure returns `None`.
- In `parse_all` (`loom/src/commands/usage/mod.rs`), inside the `Ok(mut transcript)` arm, after the `if let Some(metadata)` block and before `forward_join.join_transcript`, and only when it left `agent_type` as `None`, and only for a subagent file, set `transcript.agent_type`. Do not set `stage_id`, `loom_session_id` or `requested_model` from it.
- `mod.rs` is 393 lines and the limit is 400. Keep the addition to a `mod` line and a few lines in `parse_all`.
- Four readers of `agent_type` change under `--all`: `forward_join.rs` `is_forwarder`, `claude_provider.rs`'s `attribution.role`, `lifecycle_orientation.rs` `is_read_only` (reviewers, advisors and `Explore` move into `skipped_read_only`, so `lifecycle.subagent` medians shift), and `sections/agents.rs`. In `agents.rs`, `agent_type_ledger_matches` counts only transcripts with `agent_type_from_sidecar == false`; add `agent_type_sidecar_matches`, render it beside the ledger row, and test that a sidecar-derived type leaves the ledger count at 0. Update the doc comment on `Transcript.agent_type`. Report the effect on each reader.
- The `usage_tests.rs` cases that expect `None` when no ledger row exists. Those fixtures have no sidecar, so they should still pass. Confirm rather than assume.

## Attachments

- `transcript.rs` scans rows and `build_transcript` constructs the struct. Add `attachments` to `ScanResult` and `Transcript`.
- Real rows are `{"type":"attachment","attachment":{"type":<kind>,...},"rendered":[{"content":<string>},...],"timestamp":...}`. `kind` is `attachment.type`, and `chars` is the total character count of the `content` strings in the top-level `rendered` array. `rendered` is an array of `{content}` objects, never a string. A row without a non-empty `rendered` array is skipped: the model never saw it (`prompt_snapshot`, `hook_success`, `deferred_tools_record`, `credential_org`, `hook_permission_decision`, `command_permissions`).
- `requests_before` must count requests once, because one API response spans several rows sharing `message.id`. Count retained non-synthetic `Entry::Assistant` entries.
- Apply the same `since` cutoff as entries.
- Find every `Transcript {` literal with `rg -n 'Transcript \{' loom/src/commands/usage` and add the new fields; there are 11.

## Attribution section

Extend the scaffolded `sections/attribution/mod.rs`; `sections/mod.rs` already declares it. Add a `Report` field, `attribution::build(transcripts)` and `attribution::render(&report.attribution)`. Render it after `reads`.

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

Join tool results to tool uses through `Request.tool_uses[].id` and `UserEntry.tool_use_id`, as `sections/reads.rs::result_bytes` does. Classify by `tool.name` first. Call `reads.rs::read_path` only when the tool is `Read`, because it also returns paths for Bash `cat`, `sed -n`, `head` and `tail`, which belong in `bash:file-read`. Make `read_path` and `is_knowledge_path` `pub(super)`, and use `is_knowledge_path` for `read:knowledge`. `result_bytes` counts bytes: use `text.chars().count()` for `chars`.

`startup_by_agent` groups by `transcript.agent_type`, then `main` for main scope, then `unknown`. It reports the medians the stage description defines, keeping the 8 largest kinds per agent type. New nested types derive `Debug` and `Serialize`, because `Report` derives both.

Bounds and safety:

- Keep at most 24 attribution rows, largest first, and merge the tail into `other`.
- Attachment kinds and agent types are transcript text: pass them through `inline_safe` when rendering, as `sections/agents.rs` does.
- Label the report as an estimate. Label the replay line an upper bound, print the residual with its share of the total, and print `over_attributed_s2` and `startup_excluded` on their own lines even at zero.
- Print the `Attribution` heading even with no data, then use `no_data`, as other sections do.

Put non-contract tests in `sections/attribution/more_tests.rs` if needed. Each file stays under 400 lines and each function under 50.

## KNOWN_AGENT_TYPES

In `KNOWN_AGENT_TYPES` in `sections/agents.rs`, add `"loom-software-developer"` and change the array length to 7.
