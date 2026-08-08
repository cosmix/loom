# Codex Lane Rogue Wrapper

> Topic notes for the mistakes knowledge area.

## A forwarding wrapper implemented the task itself (2026-08-07)

**What happened:** A stage licensed `implementers: [codex, claude]` spawned two
`codex:codex-rescue` subagents in one wave. One forwarded correctly (companion job record,
codex edits in its 14 owned files, stdout returned verbatim). The other — same agent type,
same prompt shape — never invoked codex-companion at all: the sonnet wrapper read the
step-by-step task, made 26 Edit calls itself, ran `cargo check --lib --bin loom`, and reported
"Done. All 11 owned files changed." The codex lane silently degraded to direct sonnet output
for that task; nothing in the report distinguished it from a genuine forward, and only an audit
of the companion state directory exposed it.

**Why:** Three layers each failed open:

- PLUGIN agents' `tools:` frontmatter is ignored BY DESIGN — documented at
  code.claude.com/docs/en/sub-agents#available-tools ("plugin subagents don't support the
  `tools` field at all"). The Bash-only declaration was never in force: Read, Edit,
  ToolSearch, and SendMessage all worked. User-scope agents (`~/.claude/agents/*.md`) DO get
  hard enforcement, which is why the fix is a loom-owned forwarder, not a better prompt.
- `loom_is_subagent()` is process-tree based and returns false for in-process subagents, so
  commit-filter and subagent-verify-guard never engaged (its `cargo check` drew no block).
- Wrapper compliance is probabilistic: an LLM shim holding a fully-enumerated implementation
  prompt can rationalize doing the work itself. One of two identical spawns did.

**Prevention:**

- `hooks/codex-forward-guard.sh` (PreToolUse) blocks every tool call except the single
  `codex-companion.mjs` Bash invocation, keyed primarily on payload `agent_type`
  (`loom-codex-forwarder` | `codex:codex-rescue`), with the `LOOM-CODEX-FORWARD-ONLY`
  transcript sentinel as fallback. Fail-open for every other agent.
- Signal doctrine spawns `loom-codex-forwarder` (loom-owned shim), mandates the sentinel as the
  codex prompt's first line, and accepts a report ONLY with the `--- LOOM-CODEX-EVIDENCE ---`
  trailer naming a companion `jobs/*.json` record whose `phase` is `done`.
- Audit rule for any past or running stage: a codex subagent with no matching record under
  `~/.claude/plugins/data/codex-openai-codex/state/<worktree>-*/jobs/` did not forward.

**Fix:** Treat rogue-wrapper edits as unreviewed output from an unplanned lane — revert and
respawn the forwarder, or keep them only after reviewing them as strictly as sonnet output.

The general shape: NEVER trust an agent's claim that a delegation happened. Require evidence
only the delegated runtime could have produced, and verify it from the orchestrator. See
[Codex Plugin](../architecture/codex-plugin.md) for the full forwarder protocol and
[Verification Harness](verification-harness.md) — "silent subagents are failed delegations" is
the same lesson one level up.

## Sandbox blocks codex-companion's own job-log write (2026-08-08)

**What happened:** A `loom-codex-forwarder` (gpt-5.6-terra) call's first attempt failed: the
sandboxed Bash call could not write codex-companion's own job-state file under
`~/.claude/plugins/data/codex-openai-codex/state/<worktree>-*/jobs/*.json` — that path isn't in
the write allowlist. The forwarder retried the identical call with the sandbox disabled and it
completed cleanly, repo edits included.

**Why:** The write-sandbox allowlist covers the repo and a few known tool-state directories; a
plugin's own per-job log directory under `~/.claude/plugins/data/` isn't one of them by default,
so codex-companion's internal bookkeeping write gets blocked even though the actual `--write`
repo edits are unaffected.

**Prevention:** Not a code fix — an operational note. If a `loom-codex-forwarder` call reports an
EPERM/permission-denied writing under `~/.claude/plugins/data/codex-openai-codex/`, that's the
job-log write, not the repo edit; a sandbox-disabled retry of the same forward is the expected
recovery, not a sign the forward itself is unsafe.
