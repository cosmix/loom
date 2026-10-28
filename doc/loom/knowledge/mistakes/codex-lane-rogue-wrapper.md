# Codex Lane Rogue Wrapper

>

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

- `loom-hooks/codex-forward-guard.sh` (PreToolUse) blocks every tool call except the exact
  `~/.claude/hooks/loom/codex-forward.sh task ...` wrapper invocation
  (`is_exact_forward_command`), keyed primarily on payload `agent_type`
  (`loom-codex-forwarder` | `codex:codex-rescue`), with the `LOOM-CODEX-FORWARD-ONLY`
  transcript sentinel as fallback. Fail-open for every other agent. Since 2026-09-18 the guard
  engages only with stage evidence; before that it also blocked the stock `codex:codex-rescue`
  agent in every non-loom session. See
  [The forward guard engages only inside a loom stage](../architecture/codex-plugin.md).
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
back to `os.tmpdir()/codex-companion` when the var is empty. `loom-hooks/codex-forward.sh` now probes
whether `$CLAUDE_PLUGIN_DATA/state` is creatable and, only when it is not, redirects
`CLAUDE_PLUGIN_DATA` to `~/.codex/plugin-data` — inside the `~/.codex` grant this lane already
has. Machines where the default works are untouched, so the plugin's own `/codex:status` and
`/codex:result` keep finding their records.

Verified A/B on macOS 2026-09-02: the unmodified wrapper exits 1 on EPERM; with the redirect all
three tiers (`gpt-6-sol`, `gpt-5.6-terra`, `gpt-6-luna`) reach the model and exit 0. That check
stopped one layer too high: the same runs could not execute a single shell command (next entry).

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

## Verified the codex lane at the model layer, not the shell layer (2026-09-02)

**What happened:** the 67b97114 redirect was accepted on "reaches the model, exits 0"; the next
stage's five forwarders reached gpt-5.6-terra, exited 0, wrote nothing — every command codex ran,
even `pwd`, died with `sandbox-exec: sandbox_apply: Operation not permitted`.

**Why:** codex exits 0 when the model's turn ends regardless of what its tools did; the outer Bash
sandbox is Seatbelt and macOS refuses a second profile; the layer under the one just fixed was
never exercised, and the forwarders' own reports ("model invocation is fine") pointed at the layer
that worked.

**Prevention:** an end-to-end check of the lane must make codex run a command AND write a file from
a sandboxed, stage-like session, then assert on the file; treat "exit 0" from codex or the companion
as no evidence below the model. Detection rule: a codex report that reaches the model but shows
`sandbox_apply` or zero file changes is this failure, and it is the wrapper's job, never a settings
or `loom repair --fix` matter.

**Fix:** `loom-hooks/codex-forward.sh` probes for a nested Seatbelt and switches to a direct `codex exec
--sandbox danger-full-access`; details in [Codex Plugin](../architecture/codex-plugin.md) under the
2026-09-02 macOS section.

## A Backgrounded Forward Is Still Running, and Its Forwarder Must Stay Silent (2026-09-13, SYSTEMIC)

**What happened:** in all four implementation stages of PLAN-token-optimization-2026-09-13, xhigh
sol/terra forwards outran the forwarder's single 600000 ms Bash call (one module pair plus tests
was enough; runs took 24-30 minutes). The harness moved the call to a background task while the
companion job kept editing. Forwarders then ended the turn with "waiting for completion
notification" and no evidence trailer; issued a second wrapper call with a "noop" or placeholder
task, creating extra companion jobs; and twice re-ran the full task as a second writer on files
another job owned or had already settled (one re-run started 20 minutes after its unit settled; one
was cancelled by exact id before it applied anything). When the background completion woke a
forwarder, the guard blocked even `true`, so it could not relay its own trailer. The forwarder doc
meanwhile told a backgrounded forwarder to run `loom subagents wait` or `watch`, which the guard
blocks.

**Why:** the guard checked argv shape only, with no one-forward limit; the doc and the guard were
edited by different units and nothing tested the doc's commands against the guard; and prose
prevention recorded by one stage did not reach the next stage's forwarders, whose installed guard
lagged the worktree fix until merge.

**Prevention:**

- In the tree now: `codex-forward-guard.sh` allows one forward per forwarder transcript, and
  `agents/loom-codex-forwarder.md` (the background-acknowledgement rule) says a backgrounded forwarder makes no further tool call
  and ends its turn. The orchestrator alone recovers the result, through
  `loom subagents wait --receipt <id>` or the named task output. Lifecycle:
  [Token Accounting and Receipts](../architecture/token-accounting-and-receipts.md).
- Treat a backgrounded forwarder's job as alive: never respawn it or release its files until its
  exact job record is terminal.
- After a forwarder settles, look for companion jobs created after its task job and read each
  extra job's log. After cancelling a job, confirm its log has no "Applying N file change" line;
  one cancelled job was later recorded as completed.
- Keep one forward to about three items.
- When the Agent result is truncated, read the persisted task output through Bash with
  `rg -A60 '^\[codex\] Turn completed'` (Read is blocked outside the worktree) and cross-check the
  job record's phase.

## Review rounds looped because fix agents could not compile the shared checkout (2026-10-28)

**What happened:** the early-START receipt fix went through four review rounds. Each round found a defect the previous fix agent could have caught by running the tests: a test fixture that panicked, an assertion that still encoded old behaviour, and a wait that still blocked until its timeout. Another session's unfinished edit to `commands/knowledge/eval.rs` kept the shared checkout from compiling, so the fix agents edited blind and only ran rustfmt.
**Why:** verification was handed back to the main agent, and each review round then ran on code nobody had executed. A side test run under `/tmp` also filled the volume's quota and produced false failures.
**Prevention:** when the shared checkout does not build, give the fix agent a detached worktree of HEAD with the change applied and one shared `CARGO_TARGET_DIR`, and require green gates before review. Run one build at a time on the `/tmp` volume.
**Fix:** the last round went to a Fable agent working in such a worktree.

## Sandbox and relay tests fail when the checkout lives under /tmp (2026-10-28)

**What happened:** a full `cargo test` in a detached worktree under the session scratchpad failed `tests_confinement_e2e`, `tmux::tests_spawn`, `daemon_completion_contracts`, `host_verification_*` and the relay e2e tests. These failures were first blamed on the /tmp disk quota.
**Why:** loom's sandbox preflight refuses a `LOOM_BIN` or hooks directory under the session-writable root `/tmp`, and `relay_scratch.rs` refuses a scratch root under `/tmp`. The tests fail because of where the checkout is, whatever the code.
**Prevention:** judge those binaries only from a checkout and `CARGO_TARGET_DIR` outside `/tmp`. A failure whose panic names `/tmp` as the refused root is a location artifact, not a regression.

## Blamed a nonexistent concurrent session for a broken checkout

**What happened:** The pre-commit hook failed to compile loom/src/commands/knowledge/eval.rs. I attributed the edit to another live session and asked the user to wait, though no other agent was running: the edit was a half-applied held fix from finish-eval-fix.sh that duplicated HEAD's own refresh_source_graph call.
**Why:** I inferred ownership from the files being outside my change instead of checking their origin (git diff, held-fix scripts, stash list).
**Prevention:** When unrelated uncommitted edits break the build, trace where they came from before assuming a concurrent owner, and ask the user only once that trace is inconclusive. Reconciling a stash apply covers its untracked files too: `git show --stat stash^3` lists them.
**Fix:** Reconciled eval.rs against HEAD and dropped the duplicate call. That left the stash's untracked loom/src/commands/knowledge/eval/refresh.rs behind as an orphan, deleted later; HEAD's eval.rs already refreshes the graph inline.
