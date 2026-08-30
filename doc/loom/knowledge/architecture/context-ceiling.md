# Context Ceiling

> The absolute resident-token ceiling: resolution order, and the three independent thresholds (hook, daemon, native compaction) that enforce it.

## The Context Ceiling: Three Independent Thresholds, One Number

Loom stopped tracking context as a percentage of a fixed window and now tracks an
ABSOLUTE resident-token ceiling per stage/subagent. One resolved number —
`context_ceiling_tokens` — feeds three completely independent enforcement mechanisms,
each firing at a different multiple of it, from softest to hardest:

| Multiple | Mechanism                          | Who enforces it                                                                                             | What happens                                                                                                                          |
| -------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 1.0x     | `PostToolUse` hook instruction       | `hooks/post-tool-use.sh`, reading resident tokens off the transcript tail                                       | Tells the agent (via hook stdout) to finish the unit of work in progress and run a `loom handoff` with `--trigger ceiling`, then stop |
| 1.25x    | Daemon backstop (`BudgetExceeded`)   | `orchestrator/monitor/detection.rs` (`DAEMON_CEILING_MULTIPLIER`, `models/constants.rs`)                        | The daemon KILLS the session (confirmed dead via `confirm_session_gone`, not by trusting the kill call's return value) and re-queues the stage — this closes a double-spawn bug where an unconditional re-queue without a confirmed kill let two agents write the same worktree |
| 1.5x     | `CLAUDE_CODE_AUTO_COMPACT_WINDOW`    | The installed Claude Code binary itself, via an env var loom's native wrapper sets (`auto_compact_window_tokens`, `orchestrator/terminal/native/wrapper.rs:227-238`) | Claude Code's own native auto-compaction kicks in — "effectively unreachable in practice" per the source comment, because the two lower thresholds should already have ended the session by this point |

Read the resolved value in code with `fs::work_dir::resolve_context_ceiling_tokens(work_dir,
stage_ceiling)` — the one resolver. Resolution order: `stage.context_ceiling_tokens` ->
`.work/config.toml [context] ceiling_tokens` -> `DEFAULT_CONTEXT_CEILING_TOKENS` (150,000,
`models/constants.rs`); subagents default to `DEFAULT_SUBAGENT_CEILING_TOKENS` (120,000). The
1.5x auto-compact env var is separately clamped to `AUTO_COMPACT_WINDOW_MAX_TOKENS` (1,000,000)
before export, since the installed binary re-clamps to `[1, 1_000_000]` and then again to the
model's own context window.

**The true last resort is `hooks/pre-compact.sh`'s block-then-allow pattern**, independent of
all three thresholds above (it fires whenever Claude Code's native compaction actually engages,
by whatever trigger): the FIRST `PreCompact` invocation in a session drops a
`.work/compaction-pending/<session-id>` flag file, writes a handoff, and BLOCKS (exit 2) with an
instruction to record a memory note of the working state before continuing; the SECOND
invocation (flag file present) removes the flag, writes an updated handoff, and ALLOWS (exit 0).
This guarantees at least one forced context-preserving checkpoint before Claude Code discards
anything, even if all three ceiling-based mechanisms above somehow never fired.

`orchestrator/monitor/context.rs::context_health(tokens, ceiling)` is a fourth, separate
consumer of the same ceiling — it bands the ratio for DISPLAY (`loom status`, the signal's
recitation-section context line) into Green `<60%`, Yellow `60-90%`, Red `>=90%`. It does not
itself trigger anything; the 1.0x/1.25x/1.5x mechanisms above are independent of these display
bands. See [architecture.md](../architecture.md) "Context Budget Enforcement" for the full
field/resolver contract.
