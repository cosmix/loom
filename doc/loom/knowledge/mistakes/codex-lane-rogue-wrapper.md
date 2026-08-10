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

## The sandbox blocks codex's own state dirs, and the escape hatch is not an escape (2026-08-10)

**What happened:** on Linux, every codex invocation failed — in loom stages and in plain
interactive sessions alike. The sandboxed Bash call could not create codex-companion's job-state
directory (`ENOENT: ... mkdir '~/.claude/plugins/data/codex-openai-codex/state/<cwd>-<hash>/jobs'`)
and the codex CLI could not initialise its sqlite state runtime under `~/.codex`
(`Read-only file system (os error 30)`). The agent then did the documented thing — retried with
`dangerouslyDisableSandbox: true` — and the **auto-mode classifier refused it**. So there was no
path to a working codex at all: sandboxed it cannot write, unsandboxed it cannot run.

**Why:** the sandbox's write set is the working directory and the session temp dir, full stop.
Codex is a *subprocess*, not a Claude tool, and it keeps state in two dirs outside both. The
earlier note here blamed only the companion's job log and called a sandbox-disabled retry "the
expected recovery" — wrong on both counts: `~/.codex` is the bigger blocker, and the retry is a
permission decision the classifier is entitled to (and does) deny. It looked macOS-clean because
Seatbelt let those writes pass; Linux's bubblewrap sandbox enforces the allowlist.

**Prevention:** grant the two dirs at the OS layer instead of trying to leave the sandbox.
`sandbox.filesystem.allowWrite` is additive and enforced for child processes — the only lever that
reaches a subprocess. Loom emits `CODEX_SANDBOX_WRITE_PATHS` (`loom/src/codex.rs`) from the sandbox
settings generator and from `loom init`, so stage worktrees, `loom repair --fix` and freshly-
initialised repos all carry it; sessions outside a loom repo need the same block in
`~/.claude/settings.json`. See [Codex Plugin](../architecture/codex-plugin.md).

**Detection:** `Read-only file system` / `EROFS` naming `~/.codex`, or `ENOENT`/`EPERM` naming
`~/.claude/plugins/data/codex-openai-codex/`, is this — not a codex auth problem and not a reason
to distrust the forward. `codex-companion setup --json` reporting `loggedIn: false` while the same
command outside the sandbox reports `loggedIn: true` is the same cause wearing a misleading label:
the auth probe fails because codex cannot write its state runtime, not because the login expired.

**Fix:** never answer this with `dangerouslyDisableSandbox`. Report it, and fix the settings.
