# Verification Harness

> When every check fails at once, suspect the harness; the PATH binary is not your build; silent subagents are failed delegations.

## A Harness That Redirects to a Hardcoded `/tmp` Path Reports False Failures (2026-07-28)

**What happened:** an acceptance-runner script logged each command to `/tmp/acc.$$.log`. In this
sandbox `/tmp` is read-only and only `$TMPDIR` is writable, so **every redirection failed before
its command ever ran** and all 13 criteria reported FAIL while the underlying commands were
fine. The overall pipeline still exited 0, because the failure was swallowed by a `tail` pipe.

**Prevention:** use `${TMPDIR:?}` in any harness script, never `/tmp`. **Detection:** when
_every_ check in a suite fails at once, suspect the harness before the code — a real regression
is rarely uniform. This is Rule 13 (exit 0 is not success) in its most expensive form.

## The Installed PATH Binary Does Not Contain Your Plan's Changes (2026-07-28)

**What happened:** the `loom` on PATH had no `knowledge index` subcommand, because the dev
install had not been re-run since the stage that added it merged. Functional verification of a
new subcommand through the PATH binary verifies the _old_ code and calls it green.

**Why the confusion:** CLAUDE.md Rule 11 ("always use `loom` from PATH, never `target/debug`")
is about not corrupting real `.work/` state with a dev binary. It does **not** mean the PATH
binary contains your changes.

**Prevention:** run `<binary> <new-subcommand> --help` before trusting any check that uses it.
For verifying new subcommands, drive the freshly built binary; keep PATH `loom` for anything
that mutates real orchestration state.

## Silence From a Review Subagent Is a Failed Delegation, Not Something to Wait On (2026-07-28)

**What happened:** three reviewer subagents were spawned for the security, architecture, and
test dimensions. None ever returned a report; two nudges over the whole stage produced nothing
and all three had to be stopped. The delay was not noticed early enough, so those dimensions
were ultimately covered by the main agent's own adversarial passes — which is where the two real
defects were found.

**Prevention:** give a review subagent an explicit deadline and a compact output contract, check
for a reply within a bounded number of turns, and treat silence as a delegation to redo inline.
A pending subagent must never become a reason to defer the main agent's own verification.

## `rg -r` Is `--replace`, Not Recursive (2026-07-28)

**What happened:** `rg -rn <pattern> <path>` rewrote every match in the _output_, producing text
that reads as though the codebase actually contains the mangled string.

**Prevention:** `rg` recurses by default — never pass `-r` for recursion. If output looks
textually mangled, suspect `-r`.
