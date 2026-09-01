# Codex Lane Rogue Wrapper

> A forwarding wrapper that did the task itself instead of forwarding, and why the codex sandbox state-dir escape hatch is not one.

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

## The sandbox blocks codex's own state dirs, and the escape hatch is not an escape (2026-08-10)

**What happened:** on Linux, every codex invocation failed — in loom stages and in plain
interactive sessions alike. The sandboxed Bash call could not create codex-companion's job-state
directory (`ENOENT: ... mkdir '~/.claude/plugins/data/codex-openai-codex/state/<cwd>-<hash>/jobs'`)
and the codex CLI could not initialise its sqlite state runtime under `~/.codex`
(`Read-only file system (os error 30)`). The agent then did the documented thing — retried with
`dangerouslyDisableSandbox: true` — and the **auto-mode classifier refused it**.

**Why:** the sandbox's write set is the working directory and the session temp dir, full stop.
Codex is a *subprocess*, not a Claude tool, and it keeps state in two dirs outside both.

**The 2026-08-10 prevention is NO LONGER SUFFICIENT (superseded 2026-09-01).** That entry said to
grant the two dirs via `sandbox.filesystem.allowWrite` and treat the problem as solved. Verified
on macOS 2026-09-01: the grant was present and correct — `.claude/settings.local.json` carried both
`~/.codex` and `~/.claude/plugins/data/codex-openai-codex` from `CODEX_SANDBOX_WRITE_PATHS` — and
three codex forwards (sol, terra, luna) still died identically with
`EPERM: operation not permitted, mkdir '.../state/loom-<hash>/jobs'`. A bare `mkdir` under the
granted path fails the same way from an ordinary session.

The cause is a CONFLICTING rule above loom's layer, not a missing one: the harness sandbox carries
`~/.claude/plugins/data/codex-openai-codex` in its write allow-list AND `~/.claude/plugins` in its
deny-within-allow list. The broader deny shadows the narrower grant. Loom emits no deny on that
path — `rg 'claude/plugins' loom/src` returns only `codex.rs`'s grant and the companion-runtime
lookup — so `loom repair --fix` cannot fix it and re-running it changes nothing.

**Detection:** `EPERM`/`ENOENT` naming `~/.claude/plugins/data/codex-openai-codex/`, or
`EROFS` naming `~/.codex`. Before blaming loom's settings, CHECK THE GRANT IS ACTUALLY ABSENT:
read `sandbox.filesystem.allowWrite` in `.claude/settings.local.json`. If the entry is there and
the write still fails, this is the shadowing case and no amount of repair will help. A subagent
that reports "the allowWrite entry is missing" without reading the file is guessing — three did
exactly that on 2026-09-01, and all three recommended a fix that was already applied.

**Fix:** never answer this with `dangerouslyDisableSandbox`. Report it and escalate to whoever owns
the harness sandbox policy; the loom-side grant is already correct. Open follow-up: loom's codex
availability check (`codex.rs`, the CLI + plugin probe) does not test WRITABILITY of the state
directory, so a stage lists codex, starts, and every codex subagent dies mid-run instead of loom
warning at `loom run` startup.
