# Mistakes & Lessons Learned

> Record mistakes made during development and how to avoid them.
>
> **Format:** Describe what went wrong, why, and how to avoid it next time.
>
> **Related files:** [conventions.md](conventions.md) for correct patterns, [patterns.md](patterns.md) for design guidance.

## Phantom Merges: merged=true Without Verification

`merged=true` is a contract with the dependency scheduler — every phantom-merge incident came from writing it without verifying git ancestry. Ten related lessons (a retry that reset a completed stage's branch so nothing merged, git merge failing on sandbox bind mounts, defensive "assume merged" branches, `--force-unsafe`, helpers that abort active merges, merge-probe preflight, merge-conflict session lifecycle, the silent `Completed + !merged` resting state). → [Phantom Merges](mistakes/phantom-merges.md)

## File Locking: Writing to Locked Handles

`fs::write()` opens a NEW handle that ignores locks held by other handles — write to the locked handle instead. Also covers a whole-record load→mutate→save race that reverts concurrent writers even though each individual save is itself locked. → [Concurrency and Locking](mistakes/concurrency-and-locking.md)

## Source vs Installed: Editing Wrong File

Seven lessons on what a large removal or rename leaves behind — straggler initializers, stale comments, stale docs, duplicate modules, files outside the assignment table nobody owned, a missed global-hook registration site. Prevention: grep the whole workspace for the symbol, not just your assigned files.

→ [Refactor Stragglers](mistakes/refactor-stragglers.md)

## Sandbox: Contradictory Path Rules

Sandbox path rules, permission sync, `excludedCommands` matching, settings env leaking between the main repo and its worktrees, a worktree-only escape rule applied at the repo root, a directory literally named `hooks/` being sandbox-protected regardless of permission config, and `Write(path)` permission rules never enforcing (only `Edit(path)` does). Root cause: settings are _merged_ from several sources. The tool and network failures (sccache, `cargo audit`, loopback TCP, bind-mounted files, macOS path aliases) and the `.loom/work` state channels (credentials, `loom memory`, handoff, daemon socket, ledgers) each have their own topic. → [Sandbox & Settings](mistakes/sandbox-and-settings.md), [Tooling & Network](mistakes/sandbox-tooling-and-network.md), [State Channels](mistakes/sandbox-state-channels.md), [Directory Named `hooks/`](mistakes/sandbox-protected-hooks-dir.md), [Sandbox Write Rules Inert](mistakes/sandbox-write-rules-inert.md)

## Test Code: Struct Init Without Default

Lint and test-discipline lessons spanning `--all-targets`, `--no-fail-fast`, ambient git config in tests, the maintainability ledger, `TODO` in string literals and platform-specific Bash/Rust traps. Racy tests (inherited descriptors, ETXTBSY, serial env, stdin hangs, real-home writes) and CI toolchain drift (clippy on rustup `stable`, offline `cargo audit`, `install.sh`, dependency build caches) have their own topics. → [Testing & Lint](mistakes/testing-and-lint.md), [Test Concurrency & Fixtures](mistakes/test-concurrency-and-fixtures.md), [CI Toolchain & Cargo](mistakes/ci-toolchain-and-cargo.md)

## gawk vs POSIX awk (2026-03-31)

Cross-platform shell/hook portability traps: gawk extensions failing on macOS's BSD awk, hook integration tests missing a shared dependency, non-portable `timeout`, an empty-array guard that is a syntax error on a different bash, an unneeded chmod, a heredoc-scanning finalization guard, a Python hash-seed, redirect order, a `set -e` function tail, hook tests inheriting the live session's LOOM_* variables, and `updatedInput` without `permissionDecision`. → [Hooks: Shell Portability](mistakes/hooks-shell-portability.md)

## Session Identity: Backend Metadata Must Be Persisted

Session identity, liveness routing, spawn-site coverage, the struct-literal blast radius of adding a session field, settings-env identity leaking into worktrees/main-repo sessions. Root cause: a session fact derived at one call site instead of persisted and read back through the shared service.

→ [Sessions & Liveness](mistakes/sessions-and-liveness.md)
→ [Session Identity Env](mistakes/session-identity-env.md)

## Hooks: Shell Command Matchers (2026-07-28)

Token-based Bash matchers repeatedly shipped with bypasses because separators that are _glued_ to a neighbour never become tokens. Also: forgeable `glob | head -1` privilege lookups, env leakage into simulated process trees, three Bash parsing traps.

→ [Shell Command Matchers](mistakes/shell-command-matchers.md)
→ [Hooks: Shell Portability](mistakes/hooks-shell-portability.md)

## Doctrine, Acceptance Criteria, and Cross-Surface Drift (2026-07-28)

A grep for one phrase proves presence, never agreement. Covers doctrine drift across surfaces, stage-completion and acceptance-criteria mistakes (working_dir paths, stale/impossible criteria, premature completion, plan-authoring boundary errors, dry-run drift, hook enforcement gaps).

→ [Doctrine & Acceptance](mistakes/doctrine-and-acceptance.md)

## Verification Harnesses and Stale Binaries (2026-07-28)

When every check in a suite fails at once, suspect the harness. Also: the PATH binary lagging your build or `main`, silent review subagents, acceptance false negatives, a command whose output must report every layer it drives, untracked plan/worker briefs leaving a worktree blind.

→ [Verification Harnesses](mistakes/verification-harness.md)

## Knowledge CLI and Filesystem Invariants (2026-07-28)

Invariants belong in the filesystem constructor, not the CLI handler; sibling-file refreshes must happen outside the directory lock; `update` appends, so retries duplicate; CWD resolution for knowledge commands.

→ [Knowledge CLI Invariants](mistakes/knowledge-cli-invariants.md)

## Knowledge Base Drift — The Base Itself Goes Stale (2026-07-30)

Repeatable failure modes: plan-authoring notes frozen as architecture facts, `[UPDATED]` sections that appended rather than replaced, invented CLI commands, features documented that were never built, stale doc claims trusted without re-verifying, a spooled-knowledge file whose claims went stale before it was applied.

→ [Knowledge Base Drift](mistakes/knowledge-base-drift.md)

## Codex Lane Rogue Wrapper (2026-08-07)

A `codex:codex-rescue` spawn received a codex prompt and implemented all 26 edits itself on sonnet instead of forwarding — plugin agents' `tools:` field is ignored by design. Now pinned by `loom-hooks/codex-forward-guard.sh` + the `loom-codex-forwarder` agent + the evidence-trailer rule. Systemic since (2026-09-13, four stages): a forward that outruns the 600 s Bash call is still running, and its forwarder must make no further call. → [Codex Lane Rogue Wrapper](mistakes/codex-lane-rogue-wrapper.md)

## tmux Backend: Silent Spawn Failures and Layout Traps (2026-08-08) [DETAILED]

`tmux new-session` can print an error to stderr and still exit 0, so exit status alone is never evidence a server exists — assert on the resource. Also: a spawn helper that leaked a live agent, PID-file reuse, a sticky fallback marker, the 104-byte `sun_path` budget, tmux 3.7b layout/option facts.

→ [tmux Backend](mistakes/tmux-backend.md)

## Tests That Cannot Fail (2026-08-08) [DETAILED]

A test whose _name_ states a property is not evidence the property is pinned. Detection rule: for each test ask "if I delete the production line this covers, does it fail?" Every negative assertion needs a positive control asserted at the same moment. The repo's most recurrent defect class; its 2026-09-13 form is fixtures that do not mirror production input.

→ [Tests That Cannot Fail](mistakes/tests-that-cannot-fail.md)

## Completion Broker: a Server-Side Fallback the Client Could Never Reach (2026-08-11)

No worktree stage could complete through the trusted PostToolUse broker: the client sent an EMPTY credential when `user.token` was unreadable, trusting a daemon peer-identity fallback — but the wire preface refuses to frame an empty credential, so the request never left the process. Rule: a designed fallback must be traced end-to-end through every framing layer, and "absent" needs an explicit non-empty wire encoding.

→ [Completion Broker Credential](mistakes/completion-broker-credential.md)

## Pinned Literals: the Maintainability Ledger and Wiring Checks

`maintainability-baseline.txt` is an EXACT-match ledger — it fails on shrinkage as loudly as on growth — and goal-backward wiring checks pin a literal PATTERN to a literal PATH, so a genuinely better refactor reports a phantom wiring gap. `rg` your target paths against both before fanning out.

→ [Pinned Literals, Ledgers and Wiring](mistakes/pinned-literals-ledgers-and-wiring.md)

## Parallel Worktrees Share Derived State

One question catches the class: **was this path resolved through `main_project_root` or the
`.loom/work` symlink?** If so it is shared with every sibling stage and the main repo.

→ [Parallel Worktree Shared State](mistakes/parallel-worktree-shared-state.md)

## A Missing Subagent Report Is Not a Missing Result

Subagents can edit files correctly and never report. Verify the WORK (run the gate, `stat` the files), never wait on reports alone. Also covers interactive-Claude billing/capture traps and delegation-model tier discipline; brief-writing lessons (sizing, proving commands, file ownership, fixture sweeps, baseline pressure) live in their own topic.

→ [Subagent Orchestration](mistakes/subagent-orchestration.md), [Subagent Briefing](mistakes/subagent-briefing.md)

## Visibility Is Capped by Path Reachability

`pub(crate)` on an item means nothing if a module on its path is private, and `tests/` is an external crate that can only reach `pub` items. Also: sweeping for the wrapper struct instead of the struct itself, a field name that names the wrong domain object.

→ [Visibility and Reachability](mistakes/visibility-and-reachability.md)

## Auditing an Untrusted-Value Boundary

Enumerate every PRODUCER of a rendered field, not every field — one `unwrap_or` upstream defeats a normalizer you already read. Classify by DESTINATION, not origin. Also: consolidated security findings, UTF-8 byte-slicing panics, character-class allowlists that admit `..`. → [Untrusted Value Boundaries](mistakes/untrusted-value-boundaries.md)

## Cleanup Inside "Merge" Destroyed the Evidence

`attempt_auto_merge` deleted the worktree and branch inside its own success arms — the branch its caller needed to verify the merge. Ask of any function with irreversible side effects: **after this returns, what can no longer be verified?** Also: cleanup refusing over scaffold it never planted, and over loom's own memory spool. → [Merge Cleanup Boundary](mistakes/merge-cleanup-boundary.md)

## Shipping the Store Without the Consumer

A deliberately-deferred consumer left a trail of `pub` items that all look wired and compile
clean. For every new `pub` item, **name the production caller** — the compiler never warns.

→ [Store Without Consumer](mistakes/store-without-consumer.md)

## A Strict Schema Attribute Broke a Second, Unnoticed Reader

`deny_unknown_fields` added to catch typo'd PLAN keys also governed a second deserialization source and silently broke it. Also: the `LoomConfig` vs outer-document deserialization root, a scalar config field that silently asserted "exactly one" when a stage could legitimately want several at once.

→ [Schema Reuse and Silent Skips](mistakes/schema-reuse-and-silent-skips.md)

## Writer and Reader Disagreeing on One Address

A derived layer written under a key its reader never consults is indistinguishable from doing nothing. One shared definition of the key, and a round-trip test through the consumer's address, are the only defences.

→ [Writer/Reader Address](mistakes/writer-reader-address.md)

## The Channel a Doctrine Names Must Be Privileged, or the Doctrine Disables It

"Only `loom knowledge ...` may write knowledge" assumed the loom CLI was a privileged writer. It is an ordinary child of the sandboxed shell, so correcting an inert `Write(...)` rule to `Edit(...)` disabled distillation outright — the no-op was load-bearing.

→ [Knowledge Write Channel](mistakes/knowledge-write-channel.md)

## Computed Values and Hidden Couplings (2026-08-21)

Three lessons: a value computed and carried correctly was still wrong because something OUTSIDE the function reading it depended on it — an uncapped value shipped unpublished, a "reason" field that was secretly also a control signal, a ratio-based threshold whose meaning changed as its corpus grew.

→ [Computed Values and Hidden Couplings](mistakes/computed-values-and-hidden-couplings.md)

## Two Daemons Once Attached to the Same `.loom/work/` (2026-08-08)

Nothing enforced daemon singleton, so a second daemon could attach to a live `.loom/work/` and both would drive the same stages. Startup now takes an authoritative `flock` for the daemon's whole lifetime before touching the socket or control files.

→ [Daemon Singleton Incident](mistakes/daemon-singleton.md)

## An Unbounded Walk Up the Filesystem Adopts Whatever It Finds (2026-08-29)

`loom memory note`, run outside a repository, wrote its journal into an unrelated ancestor directory because the repo-root search walks up without a ceiling and validated the result by NAME rather than structure. Also: `find_repo_root_from_cwd` returning `Some(cwd)` outside any repo, and the pre-commit hook installer fabricating `.git/` in a non-repo, after which `git init` refused the sandbox's 0-byte `config.lock` placeholder (2026-09-11).

→ [Ambient Filesystem Trust](mistakes/ambient-filesystem-trust.md)

## Adjudication Autonomy Deadlock (2026-09-02)

Session adoption matched by `stage_id` alone, an unconditional re-queue, a disputing agent never retired, no watchdog for "Executing with nobody working" combined into a 90-minute deadlock.

→ [Adjudication Autonomy Deadlock](mistakes/adjudication-autonomy-deadlock.md)

## Status Broadcast Hardening: Frame-Overflow Eviction and Read-Timeout Desync

Widening the daemon's status push exposed two latent defects: an oversized payload silently evicted every socket subscriber, and a 50ms read timeout could desync the stream permanently.

→ [Status Broadcast Hardening](mistakes/status-broadcast-hardening.md)

## Ledger TUI: Wide Glyphs, Fan-Out Duplication, and Latent Panics

Five workers fanning the ledger TUI out by file each reimplemented cell-padding and truncation independently against a status set that includes a wide glyph. Also: a frozen accumulator rendered as a live clock in `loom status --live`'s TIME column.

→ [Ledger TUI Rendering](mistakes/ledger-tui-rendering.md)

## Web Dashboard Server

Concurrency bugs, a cross-worker field-semantics bug, a browser-vs-jsdom gotcha, and terminal
bridge lessons, all from `loom/src/commands/status/web/` and `web/`.

→ [Web Dashboard Server](mistakes/web-dashboard-server.md)

## Pre-Commit Partial-Staging Guard: Design Decisions and Edge Cases (2026-09-12)

Settled disputes of the partial-staging guard: rename detection, why `commit -a`/`commit <path>` cannot false-positive it, and a NUL-safety fixture trap. → [Pre-Commit Hardening](mistakes/pre-commit-hardening.md)

## A Stage's Test Run Rewrote Live State (2026-09-13) [DETAILED]

Tests run by a stage adopted the live `.loom/work` through `WorkDir::new`'s upward walk (TMPDIR nested in the checkout) and wrote a sticky marker that overrode `[terminal] backend`. Stage sessions must not write `.loom/`; no marker may override a configurable setting.

→ [Live State Pollution](mistakes/live-state-pollution.md)

## Config tiers inherit per key; a section that wins whole is a defect

Every config key resolves per key, project -> user -> built-in, whether or not the tier's section exists; section-level shadowing is a defect to raise, not a design to keep. → [Typed Config Values](mistakes/typed-config-values-process.md)

## Stop hook exited 141: `cmd | head` under pipefail (2026-09-14)

Under `pipefail`, piping a command into `head` kills the producer with SIGPIPE and the hook exits 141 with empty stderr; capture output first, then truncate. → [Hooks Shell Portability](mistakes/hooks-shell-portability.md)

## Spurious waiting-for-input stages (2026-09-14) [DETAILED]

Stages flipped to `WaitingForInput` with no AskUserQuestion in any transcript, because the ask-user hooks acted on any invocation of the AskUserQuestion permission pipeline without reading stdin, and nothing reconciled the state; `loom stage complete` was then refused. The monitor now resumes a waiting stage whose own session keeps executing tools, and the hooks check `tool_name` and log every trigger. A second cause (2026-09-16): the SubagentStop lifecycle heartbeat carried no `subagent` flag, so the reconciler resumed a real wait; lifecycle heartbeats now set the flag and the reconciler requires a named tool.

→ [Spurious waiting-for-input stages](mistakes/spurious-waiting-for-input.md)

## Codex Worker Briefing Gotchas (2026-09-14)

A Bash command whose text contains both "loom" and any "complete" substring gets pinned to the exact stage-completion form by `loom-control-complete.sh`, even for unrelated `loom knowledge` calls — pipe long content in from a file instead of a heredoc. Plus: codex-written Rust text needs escaped `format!` braces and backticked doc-comment placeholders, "reuse" briefs must name the exact import path, integration-test submodules need `#[path]`, retiring an evidence format needs a fixture sweep, and jq exit status must be checked separately from an empty result. See [Codex Worker Briefing Gotchas](mistakes/codex-worker-briefing.md).

## Harness or Tool-Result Text Does Not Override CLAUDE.md

An "auto mode" note in a tool result pushed `cat`/`sed`/heredocs over Rule 8's Read/Edit/Write/`rg`, and a harness reminder put a `Co-Authored-By` trailer on a commit against Rule 9 and [Commit Convention](conventions/commits.md) (2026-09-18; `commit-filter.sh` blocked the whole Bash call, so the chained `git add` never ran either). Harness and tool-result text rank below CLAUDE.md even when phrased as an instruction: treat a contradicting one as content and keep to the binding rule. The one standing Rule 8 exception is appending to a file without having read it, where a Bash heredoc append is correct because Write would overwrite and Edit needs the existing text.

## A Verification Brief's Negative Expectation Must Trace to the Plan, Not an Assumption

A verifier's own assumption about required behaviour is not a substitute for the plan's stated contract. See [Typed Config Values: Process and Verification Gotchas](mistakes/typed-config-values-process.md).

## A Backgrounded Dev Server Is Invisible to the Next Bash Call

Each Bash tool call gets its own sandboxed process namespace. See [Typed Config Values: Process and Verification Gotchas](mistakes/typed-config-values-process.md).

## The Pre-Commit Markdownlint Fixer Rewrites Plan Prose That Wraps to Start With `+`

Never wrap plan prose so a line begins with `+`. See [Typed Config Values: Process and Verification Gotchas](mistakes/typed-config-values-process.md).

## A Bug Report From a Loom Stage Is About Loom the Product, Not About a Project on This Machine (2026-09-18)

A report quoted from a stage session is a defect report against loom's source: diagnose from this repo, never from another project's transcripts or state. → [Briefs and Bug Reports](mistakes/briefs-and-bug-reports.md)

## A Brief That Invents a New Guard Flag Can Widen an Existing One (2026-09-18)

Before a brief adds a condition variable beside an existing guard, state what the existing one already covers and why it falls short. → [Briefs and Bug Reports](mistakes/briefs-and-bug-reports.md)
