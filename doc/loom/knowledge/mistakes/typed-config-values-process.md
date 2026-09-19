# Typed Config Values: Process and Verification Gotchas

> Verification-brief, dev-server, plan-prose gotchas
> itself — verification-brief authorship, local dev-server checks, and plan-file formatting.

## A Verification Brief's Negative Expectation Must Trace to the Plan, Not an Assumption

**What happened:** an integration-verify functional-verifier brief demanded that a fixture with
`update.check`'s user value as the string `"true"` fail `configResponseSchema.parse`, and reported a
FAIL when it didn't. The plan's own brief (`w4-web-model.md:44`) specified `configValueSchema` as a
kind-agnostic `z.union`, which parses that fixture successfully by design.

**Prevention:** before writing a negative expectation into a verification brief, check that the plan
or worker brief actually states that behaviour is required — a verifier's own assumption about what
"typed" should mean is not a substitute for the plan's stated contract.

## A Backgrounded Dev Server Is Invisible to the Next Bash Call

**What happened:** `loom status --web &` started in one Bash tool call could not be reached from a
later Bash call in the same session — `ps`/`curl` in the follow-up call saw nothing, and `curl` got
connection refused.

**Why:** each Bash tool invocation gets its own sandboxed process namespace; a background process
started in one call does not survive into the next.

**Prevention:** start the server, wait for its port, `curl` it, and kill it — all inside ONE Bash
command.

## The Pre-Commit Markdownlint Fixer Rewrites Plan Prose That Wraps to Start With `+`

**What happened:** a plan-prose line that happened to wrap so its continuation began with `+` was
silently rewritten into a Markdown list item (with a blank line inserted) by the pre-commit
markdownlint fixer, leaving an unstaged edit to the plan file after every commit.

**Prevention:** never wrap prose so a line begins with `+`. After committing a plan file, check
`git status` for an unstaged fixer edit and revert it if the rewrite was not intended.

## Config tiers inherit per key; a section that wins whole is a defect

**What happened:** The settings dialog showed `native` on the project lane of `terminal.backend` while the user tier set `tmux` and the project config had no `[terminal]` section. The proposed fix made the section-absent case fall through to the user tier but kept a present-but-keyless `[terminal]`/`[context]` section deriving the built-in, preserving the documented section-level shadowing.
**Why:** `conventions/model-and-effort-config.md#key-level-vs-section-level-fallback` and the `config_api/workspace.rs` doc comments describe section-level shadowing as deliberate; the proposal took that as a constraint instead of checking it against the precedence the operator expects.
**Prevention:** Every config key resolves per key, project -> user -> built-in: a tier that does not set a key resolves to, and displays, the next tier down, whether or not its section exists. A rule that derives built-ins for a key a present section omits is a defect to raise with the operator, not a design to preserve.
**Fix:** Operator decision 2026-09-13: `[terminal]` and `[context]` move to key-level fallthrough in the runtime resolvers (`fs/work_dir/config_sections.rs`) and in `/api/config` (`config_api/workspace.rs`, `entries.rs`).
