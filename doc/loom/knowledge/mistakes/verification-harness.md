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

## The PATH Binary Can Lag `main` MID-PLAN, Not Just Behind Your Build

Rule 11 says always use `loom` from PATH, never `target/debug/loom`. That is right for
avoiding state corruption, but it makes `--help` an unreliable source of truth for
documenting the tree you are working in.

**Observed at the end of the context-retrieval plan**, from one installed binary:

```text
loom knowledge context --help   -> works, full new flag set
loom map --help                 -> shows only --deep --focus --overwrite
loom context record-edit --help -> error: unrecognized subcommand 'context'
loom hook user-prompt --help    -> error: unrecognized subcommand 'hook'
```

All four commands exist in the source. The binary had been reinstalled after the
`context-core` stage merged and before `source-graph` and `delivery` merged, so it carried
exactly the first stage's surface. Nothing about the output says "stale" — a missing
subcommand looks identical to a subcommand that was never written.

**Prevention:**

- When documenting or verifying a CLI surface, read the clap definitions
  (`loom/src/cli/types.rs`, `cli/types_ops.rs`, the command's own `Args` struct), and treat
  `--help` as corroboration only.
- An "unrecognized subcommand" for something you can see in the source means a version
  mismatch, not a missing feature. Check `git log` for when it merged versus when the binary
  was installed.
- This is the same family as the existing entry about a PATH binary not being your build; the
  new part is that a MID-PLAN reinstall makes the mismatch partial and therefore convincing.

## A Sandboxed Bash Tool Hides Other Processes, So Loom Reports Live Sessions as Dead (2026-08-19)

**What happened:** while diagnosing a `loom attach` complaint, `loom attach` run from the agent's
sandboxed Bash printed `No live tmux sessions` for a session that was demonstrably alive — its
`claude` PID, its `tmux: server` process and its socket had all been confirmed present moments
earlier. The identical command run unsandboxed found the session immediately. The first reading was
nearly filed as a loom discovery bug.

**Why:** the sandbox restricts the process table. `ps aux` inside it returned five rows — the
agent's own processes — and `pgrep tmux` found nothing while a real tmux server was running. Loom's
liveness rule is verified process identity (`TmuxBackend::is_session_alive` →
`process::ProcessIdentity`), so when the recorded PID is invisible, `live_tmux_sessions` filters out
every live tmux session and every `.work/sessions/*.md` looks stale.

**The error is one-directional, which is what makes it convincing:** a filtered process table can
only turn live into dead, never dead into live. The false reading therefore arrives as a plausible,
specific, _quiet_ answer — "no live sessions" — rather than as anything resembling a malfunction.

**Detection:** `ps aux | rg -c .` returning a handful of rows means you are reading a filtered
process table, not an idle machine. Once that is established, every loom output derived from PID
liveness — `loom attach`, `loom status`, crash detection, session listings — is meaningless in that
shell.

**Prevention:** never conclude that a session is dead, crashed, or orphaned from inside a sandboxed
shell. Establish that the process table is real first, and re-run that one command unsandboxed
before drawing any conclusion. Same family as the entries above: the tool answered honestly about
the world it could see, and that world was not the machine.

## Write Acceptance Criteria From Inside a Sandboxed Worktree, Not From Your Checkout

Every criterion below looked green and was wrong, and all four failed the same way:
they were authored from the main checkout, where `.work` is a real directory and the
derived cache is writable. In a stage worktree `.work` is a SYMLINK to the main repo
and the plan sandbox denies writes to it, so any criterion whose command writes a
derived cache behaves differently there than where it was written.

| Criterion as written | What actually happens in a stage worktree |
| --- | --- |
| `loom map --outline src/main.rs \| rg -q function` | unsatisfiable — `loom map` called `reconcile_source_graph`, which WRITES an overlay under `.work/context`, so every invocation hard-failed with `Read-only file system (os error 30)` even though a readable base layer existed |
| `loom knowledge sync --json \| rg -q '"semantic":{'` | cannot fail — the denied write returns exit 0 with `{"semantic":{"layer":"skipped",...}}`, so the key is present on a sync that did nothing |
| `$L init >/dev/null 2>&1 \|\| true` then check layers | cannot pass — `loom init` REQUIRES a `<PLAN_PATH>` and exits 2; `\|\| true` turns the usage error into a silent zero-result |
| `rg --files doc/plans/PLAN-x.md > /dev/null && ...` | fails on an absent file — a worktree materialises only TRACKED files, and those sibling plans were untracked |

**Prevention, in the order the failures appear:**

1. **Run every CLI acceptance criterion from inside a stage worktree with the plan
   sandbox ON before shipping the plan.** "Works in my checkout" is not evidence; the
   stage sandbox is the primary environment for these commands.
2. **A read-only CLI verb must degrade when its derived cache is unwritable**, the way
   `context/retrieve.rs:87` `resolve_catalog` already does. `loom map` is documented as
   a read-only view and was writing on every call — that is the bug the criterion
   exposed, not a criterion problem.
3. **Grep for the VALUE that proves work happened, never for a key the degraded path
   also emits.** `'"layer":"base"'` or a non-zero node count, not `'"semantic":{'`.
4. **A wiring test that invokes a CLI verb must pass that verb's required arguments**,
   and must not wrap it in `|| true`.
5. **`git ls-files <path>` every file a stage is told to read or edit, at plan time.**
   An untracked file is invisible to every worktree stage.

**And know that the escape hatch is shut.** A stage's dispute-criteria command — the only
channel an agent has for "this criterion is impossible" — authenticates over daemon RPC
by reading `.work/user.token`, which the generated stage settings put in `denyRead`. It
dies with `Failed to read .work/user.token for daemon authentication` before any RPC. So
an agent facing an unsatisfiable criterion has no structured escape and falls back to
finishing the stage as CompletedWithFailures, which auto-retries a stage whose criteria no
retry can ever satisfy. When you hit one: say so explicitly in the finishing report and
name the stage-amend operator command as the fix (`commands/stage/amend.rs`, added for
exactly this) — do NOT keep working the stage, and never quietly rewrite your own gate to
green.
