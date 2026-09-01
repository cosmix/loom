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

**What happened:** codex invocations fail before any model call because the sandboxed Bash call
cannot create codex-companion's job-state directory
(`EPERM/ENOENT: mkdir '~/.claude/plugins/data/codex-openai-codex/state/<slug>-<hash>/jobs'`), or
because the codex CLI cannot initialise its sqlite state runtime under `~/.codex`
(`Read-only file system`). Retrying with `dangerouslyDisableSandbox` is refused by the auto-mode
classifier, so it reads as having no way out.

**The 2026-08-10 prevention was INCOMPLETE (superseded 2026-09-02).** It said to grant both dirs via
`sandbox.filesystem.allowWrite`. That is necessary but not sufficient: on macOS 2026-09-02 the
grant was present and correct — `CODEX_SANDBOX_WRITE_PATHS` had put both paths in
`.claude/settings.local.json` — and three forwards still died on the same mkdir. A bare `mkdir`
under the granted path fails too, while `~/.codex` (the other granted path) is writable. The cause
is a CONFLICTING rule above loom: the harness allows `~/.claude/plugins/data/codex-openai-codex`
and denies `~/.claude/plugins` around it, and the deny on the parent wins. Loom emits no deny
there, so `loom repair --fix` cannot help and re-running it changes nothing.

**Root cause and the fix loom ships.** The companion derives its state root from an env var:
`stateRoot = $CLAUDE_PLUGIN_DATA/state` (plugin 1.0.6, `scripts/lib/state.mjs:9,41-42`), falling
back to `os.tmpdir()/codex-companion` when the var is empty. `hooks/codex-forward.sh` now probes
whether `$CLAUDE_PLUGIN_DATA/state` is creatable and, only when it is not, redirects
`CLAUDE_PLUGIN_DATA` to `~/.codex/plugin-data` — inside the `~/.codex` grant this lane already
has. Machines where the default works are untouched, so the plugin's own `/codex:status` and
`/codex:result` keep finding their records.

Verified A/B on macOS 2026-09-02: the unmodified wrapper exits 1 on EPERM; with the redirect all
three tiers (`gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna`) reach the model and exit 0.

**Platform note.** This is not simply 'macOS is stricter'. The 2026-08-10 entry recorded the
opposite — Seatbelt let these writes pass while Linux bubblewrap enforced the allowlist. The
deciding variable is the built-in deny list of the Claude Code build doing the spawning, not the
kernel sandbox. A Linux box on a build without the `~/.claude/plugins` deny works with no
redirect; the conditional probe is what makes one wrapper correct on both.

**Detection:** `EPERM`/`ENOENT` naming `~/.claude/plugins/data/codex-openai-codex/`, or `EROFS`
naming `~/.codex`. Before blaming loom's settings, CHECK THE GRANT IS ACTUALLY ABSENT by reading
`sandbox.filesystem.allowWrite` in `.claude/settings.local.json`. If it is present and the write
still fails, this is the shadowing case. A subagent reporting 'the allowWrite entry is missing'
without reading the file is guessing — three did exactly that on 2026-09-01 and all three
recommended a fix that had already been applied.

**Never** answer this with `dangerouslyDisableSandbox`.

**Open follow-up:** loom's codex availability check (`codex.rs`) probes the CLI and the plugin but
not WRITABILITY of the state root, so a stage can list codex, start, and lose every codex subagent
mid-run instead of `loom run` warning at startup.
