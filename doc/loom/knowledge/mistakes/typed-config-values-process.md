# Typed Config Values: Process and Verification Gotchas

> Brief, dev-server gotchas
> itself — verification-brief authorship, local dev-server checks, and plan-file formatting.

## A Verification Brief's Negative Expectation Must Trace to the Plan, Not an Assumption

**What happened:** An integration-verify functional-verifier brief demanded that a fixture with `update.check` set to the string `"true"` fail `configResponseSchema.parse`, and reported a failure when it did not. The plan worker brief specified `configValueSchema` as a kind-agnostic `z.union`, which accepts that fixture by design. The former source pointer to the removed worker brief is historical evidence, not an existing repository path.

**Why:** The verification brief assumed that a typed value must also be checked against the configuration key kind.

**Prevention:** Before writing a negative expectation, verify that the plan or worker brief requires that behavior. An assumption about what typed should mean does not establish the contract.

**Fix:** Align the negative expectation with the declared schema contract and remove the obsolete source pointer from this historical entry.

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

## `as const satisfies` Narrows an Optional Field Away and Fails tsc (2026-10-15)

**What happened:** web-insights typed `MIX` in `web/src/components/token-flow-panel.tsx:32` as `as const satisfies ReadonlyArray<{hint?: string}>`. Destructuring `hint` from the narrowed union failed `tsc` with TS2339.

**Why:** the worker skipped the typecheck under the subagent rule of one narrow check.

**Prevention:** a const array whose optional field sits on only some entries takes an explicit type annotation, not `as const satisfies`. Run the typecheck before reporting a TSX unit done.

**Fix:** `MIX` is annotated.
