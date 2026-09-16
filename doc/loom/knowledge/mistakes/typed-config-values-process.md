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
