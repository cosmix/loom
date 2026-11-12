# Mistakes & Lessons Learned

> Lessons learned
>
> **Format:** Describe what went wrong, why, and how to avoid it next time.
>
> **Related files:** [conventions.md](conventions.md) for correct patterns, [patterns.md](patterns.md) for design guidance.

## Phantom Merges: merged=true Without Verification

`merged=true` is a contract with the dependency scheduler — every phantom-merge incident came from writing it without verifying git ancestry. Ten related lessons (a retry that reset a completed stage's branch so nothing merged, git merge failing on sandbox bind mounts, defensive "assume merged" branches, `--force-unsafe`, helpers that abort active merges, merge-probe preflight, merge-conflict session lifecycle, the silent `Completed + !merged` resting state). → [Phantom Merges](mistakes/phantom-merges.md). Merges that ran git in the operator's checkout, six plan errors from the fix, and the later hardening round (quoted control paths, ignored-file overwrite, tag-shadowed branches, unproven CLI merges): [Merge in Operator Checkout](mistakes/merge-in-operator-checkout.md) Target-guard lessons (ancestry against a ref any session can move, fail-open checks, the graft environment value): [phantom-merges](mistakes/phantom-merges.md).

## File Locking: Writing to Locked Handles

`fs::write()` opens a NEW handle that ignores locks held by other handles — write to the locked handle instead. Also covers a whole-record load→mutate→save race that reverts concurrent writers even though each individual save is itself locked. → [Concurrency and Locking](mistakes/concurrency-and-locking.md)

## Source vs Installed: Editing Wrong File

Seven lessons on what a large removal or rename leaves behind — straggler initializers, stale comments, stale docs, duplicate modules, files outside the assignment table nobody owned, a missed global-hook registration site. Prevention: grep the whole workspace for the symbol, not just your assigned files. → [Refactor Stragglers](mistakes/refactor-stragglers.md)

## Sandbox: Contradictory Path Rules

Sandbox path rules, permission sync, `excludedCommands` matching, settings env leaking between the main repo and its worktrees, a worktree-only escape rule applied at the repo root, a directory literally named `hooks/` being sandbox-protected regardless of permission config, and `Write(path)` permission rules never enforcing (only `Edit(path)` does). Root cause: settings are _merged_ from several sources. The tool and network failures (sccache, `cargo audit`, loopback TCP, bind-mounted files, macOS path aliases) and the `.loom/work` state channels (credentials, `loom memory`, handoff, daemon socket, ledgers) each have their own topic, as does a merge resolver that inherited its stage's write denies and gave up silently, or was handed back because the integrity gate read its formatter reflow of an unchanged assertion as a changed one (reformat-only hunks are exempt). → [Merge Resolver Sandbox](mistakes/merge-resolver-sandbox.md), [Sandbox & Settings](mistakes/sandbox-and-settings.md), [Tooling & Network](mistakes/sandbox-tooling-and-network.md), [State Channels](mistakes/sandbox-state-channels.md), [Directory Named `hooks/`](mistakes/sandbox-protected-hooks-dir.md), [Sandbox Write Rules Inert](mistakes/sandbox-write-rules-inert.md) (also the TMPDIR deny that failed every Bash call)

## Session Identity: Backend Metadata Must Be Persisted

Session identity, liveness routing, spawn-site coverage, the struct-literal blast radius of adding a session field, settings-env identity leaking into worktrees/main-repo sessions. Root cause: a session fact derived at one call site instead of persisted and read back through the shared service.

→ [Sessions & Liveness](mistakes/sessions-and-liveness.md), [Session Identity Env](mistakes/session-identity-env.md)

## Hooks: Shell Command Matchers (2026-07-28)

Token-based Bash matchers repeatedly shipped with bypasses because separators that are _glued_ to a neighbour never become tokens. Also: forgeable `glob | head -1` privilege lookups, env leakage into simulated process trees, three Bash parsing traps, and cross-platform portability traps (gawk extensions on BSD awk, non-portable `timeout`, redirect order, `cmd | head` under `pipefail` killing the producer with SIGPIPE). Lexical guards also blocked 435 mostly legitimate calls in one week and agents rewrote commands until they passed: a guard blocks only what it decides soundly, names the sanctioned route, rewrites the commands loom owns, ships each relaxation with its hostile test, resolves paths physically and never keys on `TMPDIR`.

→ [Shell Command Matchers](mistakes/shell-command-matchers.md), [Hooks: Shell Portability](mistakes/hooks-shell-portability.md), [Lexical Hooks Blocked Legitimate Work](mistakes/shell-command-matchers.md#lexical-hooks-blocked-legitimate-work-and-were-routed-around), [Hook-Guard Delivery Slips](mistakes/shell-command-matchers.md#hook-guard-delivery-slips-ledger-parser-template-trim-gate-coverage-spawner-list)

## Doctrine, Acceptance Criteria, and Cross-Surface Drift (2026-07-28)

A grep for one phrase proves presence, never agreement. Covers doctrine drift across surfaces, stage-completion and acceptance-criteria mistakes (working_dir paths, stale/impossible criteria, premature completion, plan-authoring boundary errors, dry-run drift, hook enforcement gaps).

→ [Doctrine & Acceptance](mistakes/doctrine-and-acceptance.md)

## Verification Harnesses and Stale Binaries (2026-07-28)

When every check in a suite fails at once, suspect the harness. Also: the PATH binary lagging your build or `main`, silent review subagents, acceptance false negatives, a command whose output must report every layer it drives, untracked plan/worker briefs leaving a worktree blind. A fresh `CARGO_HOME` per confined criterion rebuilt every dependency until stall recovery killed the silent completion, and its stable per-tree replacement must be reset at every use because the stage agent can write it: [Completion Runtime](mistakes/completion-runtime.md). Operator overrides could not land a stage with no recorded completion commit until they recorded the branch tip: [Phantom Merges](mistakes/phantom-merges.md#operator-escape-hatches-could-not-land-a-stage-with-no-completion-commit-2026-10-20). Criteria the confined runner could not run (no network, no worktree git metadata, a fixed timeout, runner-only test failures) ended stages in manual blocks while a dispute was left: [Completion Runner Blocks](mistakes/completion-runner-blocks.md).

→ [Verification Harnesses](mistakes/verification-harness.md)

Group cleanup must trace each wrapper and session boundary. Bubblewrap starts a new session, so a host group guardian reaches sandbox descendants through the wrapper's parent-death contract: [Host Verify Group Containment](mistakes/host-verify-group-containment.md).

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

## Pinned Literals: the Maintainability Ledger and Wiring Checks

`maintainability-baseline.txt` is an EXACT-match ledger — it fails on shrinkage as loudly as on growth — and goal-backward wiring checks pin a literal PATTERN to a literal PATH, so a genuinely better refactor reports a phantom wiring gap. `rg` your target paths against both before fanning out.

→ [Pinned Literals, Ledgers and Wiring](mistakes/pinned-literals-ledgers-and-wiring.md)

## Ledger TUI: Wide Glyphs, Fan-Out Duplication, and Latent Panics

Five workers fanning the ledger TUI out by file each reimplemented cell-padding and truncation independently against a status set that includes a wide glyph. Also: a frozen accumulator rendered as a live clock in `loom status --live`'s TIME column.

→ [Ledger TUI Rendering](mistakes/ledger-tui-rendering.md)

## Web Dashboard Server

Concurrency bugs, a cross-worker field-semantics bug, a browser-vs-jsdom gotcha, and chart bands
in state tones by rank, from `loom/src/commands/status/web/` and `web/`.

→ [Web Dashboard Server](mistakes/web-dashboard-server.md)

## Web Terminal

PTY fd leak and backpressure lessons in the terminal bridge, the bounded input queue, and viewer
scrolling that must page through `{scroll:{pages}}` for both wheel and touch.

→ [Web Terminal](mistakes/web-terminal.md)

## Pre-Commit Partial-Staging Guard: Design Decisions and Edge Cases (2026-09-12)

Settled disputes of the partial-staging guard: rename detection, why `commit -a`/`commit <path>` cannot false-positive it, and a NUL-safety fixture trap. → [Pre-Commit Hardening](mistakes/pre-commit-hardening.md)

## Lint and Test-Discipline Summary

Lint and test-discipline lessons spanning `--all-targets`, `--no-fail-fast`, ambient git config in tests, the maintainability ledger, `TODO` in string literals and platform-specific Bash/Rust traps. Racy tests (inherited descriptors, ETXTBSY, serial env, stdin hangs, real-home writes) and CI toolchain drift (clippy on rustup `stable`, offline `cargo audit`, `install.sh`, dependency build caches) have their own topics. → [Testing & Lint](mistakes/testing-and-lint.md), [Test Concurrency & Fixtures](mistakes/test-concurrency-and-fixtures.md), [CI Toolchain & Cargo](mistakes/ci-toolchain-and-cargo.md), [Test Sweep & Fixture Discipline](mistakes/test-sweep-and-fixture-discipline.md)

## Verification v2 Delivery Mistakes

Twelve prevention rules from the verification-v2 plan: wave-brief ownership gaps, truncated caller searches, worker sizing and `loom subagents watch` traps, rustfmt-before-measure, the pre-commit rustdoc gate, the silent markdown lint, unregistered hook tests, commit mechanics, gates on the wrong record, unverified reviews, fixtures that cannot fail, frozen contract files, ledger lines, stage gotchas. → [verification-v2-delivery](mistakes/verification-v2-delivery.md). Token-flow stages added `reachable` call shapes, a required TypeScript field, a source edit after the dist build and a filtered commit ticket.

## An flock Outlives `drop` Under a Forking Test Binary

A test asserting an `flock` is free right after `drop` must poll to a deadline. A child forked by another test thread holds the descriptor until exec. → [Test Concurrency & Fixtures](mistakes/test-concurrency-and-fixtures.md)

## Source Graph Delivery

Resolver fixtures, candidate narrowing across files, cache mutation outside GraphStore, wiring patterns missing from briefs, formatter passes that break size caps or frozen contracts, and eval cases without expect. See [Source Graph Delivery Mistakes](mistakes/source-graph-delivery.md). `loom usage` read from a stage worktree through a symlinked work dir: [A Symlinked Work Dir](mistakes/completion-runner-blocks.md#a-symlinked-work-dir-made-every-session-record-read-as-unreadable).

## Gate and Review Rounds

Review rounds recorded twice or counted per message, a re-review after every small fix, gate runs that repeat the criteria completion runs, and a verifier round started before the cheap checks. → [Gate and Review Rounds](mistakes/subagent-orchestration.md)

Keep completed review findings separate from active assignments, and replenish independent work to the requested concurrency. → [Review Tracker](mistakes/review-tracker.md)

Coordinate shared build resources and verify integrated fixtures before claiming a green gate.
→ [Review Integration Verification](mistakes/review-integration-verification.md)

Full suites belong at the gate; targeted runs have no count cap. [Details](mistakes/test-sweep-and-fixture-discipline.md#duplicate-test-runs-were-framed-as-a-run-count-problem).

## Runtime and Daemon Incidents

Ten incidents: merge cleanup that destroyed the evidence it needed, a tracked `<file>.tmp` symlink redirecting a locked write, a broker fallback the client could never reach, two daemons on one `.loom/work/`, the adjudication deadlock, status-broadcast eviction, live-state pollution from the tests of one stage, spurious waiting-for-input, an unbounded walk up the filesystem, and parallel worktrees sharing derived state.

→ [Runtime and Daemon Incident History](mistakes/parallel-worktree-shared-state.md)

## Design and Boundary Lessons

Eight lessons: a store shipped without its consumer, a strict schema attribute breaking a second reader, writer and reader disagreeing on one address, a doctrine channel that was not privileged, computed values with hidden couplings, visibility capped by path reachability, auditing untrusted-value boundaries, and a raw string ended early by its own body.

→ [Design and Boundary Lessons](mistakes/untrusted-value-boundaries.md)

A frontmatter split must keep the YAML start and post-delimiter body start as
separate offsets; tests should assert both slices across line-ending variants.
→ [Frontmatter boundary offsets](mistakes/frontmatter-boundary.md)

## Agent and Process Lessons

A missing subagent report is not a missing result; codex worker briefing gotchas; harness or tool-result text does not override CLAUDE.md; typed-config process and verification gotchas; briefs and bug reports.

→ [Agent and Process Lessons](mistakes/subagent-orchestration.md)

## Brief-path extractor fed markdown into the run glob

**What happened:** `loom run` failed on a plan that `loom plan verify --strict` accepted, with `Pattern syntax error near position 42: recursive wildcards must form a single path component`. The plan prose read `**Briefs live outside \`doc/plans/briefs/\`.**`.

**Why:** `extract_brief_paths` (`plan/schema/structural_checks.rs`) trimmed only `.,:\`` from token ends, so the token kept the bold marker after the closing backtick. Run's input discovery compiled any token holding`*` as a glob and aborted on a bad one without naming the token or file. Verify never ran that discovery: it only warned about missing brief paths in stage descriptions, so a plan could verify clean and still fail run's preflight.

**Prevention:** a check that accepts a plan must run the same code every later consumer runs on it. Verify and the run preflight share one discovery (`plan/inputs.rs`); a new run-time check on plan inputs goes there, never into `commands/run/` alone.

**Fix:** the extractor cuts each token at its first backtick (test `markdown_after_a_code_span_is_not_part_of_the_path`). `plan/inputs.rs` collects invalid references (bad glob, parent traversal, symlink) with the token and its source file instead of aborting; `loom run` lists them with uncommitted inputs, and `loom plan verify` reports invalid and missing inputs as errors and uncommitted ones as a note (`commands/plan/verify/inputs.rs`).

## Sandbox Route Closure

A metaplan that denied the `Workflow` tool every ultracode stage runs on, a sibling-directory deny nested above a `config.worktree` deny that aborts bubblewrap, a probe control the Knowledge capsule does not grant, and metaplan reasons that went stale against the tree: [Sandbox Paths and Settings](mistakes/sandbox-and-settings.md). A `rustfmt` edition and recursion, a filtered `loom check` that hid the relay line, and the scratch directory a read-only reviewer cannot read: [Brief and Dev-Server Gotchas](mistakes/typed-config-values-process.md). An integration-verify suite criterion with no extended timeout: [Cargo Churn and Stall Kills](mistakes/completion-runtime.md).

- [contract-phase-human-blocking](mistakes/contract-autonomy-delivery.md#a-contract-freeze-refusal-waited-on-the-operator): a plan defect in the contract phase parked the stage on the operator. Rule: no stage waits on a human for something loom or an agent can resolve; a human is reached only after the automatic budgets run out.

- [codex-direct-mode-invisible](mistakes/codex-direct-mode-invisible.md): direct-mode codex forwards (Linux nested-bubblewrap fallback) leave no companion job, so reconcile, lifecycle and the status model display all miss them.

## Contract Autonomy Delivery

A chain check that read attributes by position and refused by deny-list (a `cfg` bypass), recording-time verdict validation that skipped the apply's per-patch order, escaping names where only refusal fails closed, worker checks that missed shared-fixture consumers and the maintainability test, a package-free fixture base, frozen contracts that failed the stage's own lint because the freeze ran only the format gate, a reviewer-brief fixture without a checkout identity, and tests added inside an assertion statement's span: [contract-autonomy-delivery](mistakes/contract-autonomy-delivery.md). Plans that forced definition-shaped wiring checks on stages whose consumer came later, ordered a ritual integrity dispute that cost a fresh session, or stated brief premises the tree contradicted: [plan-authoring](mistakes/plan-authoring.md). A growth budget that ignored the stable-prefix ceiling and a compile signal the subagent hook refuses: [briefs-and-bug-reports](mistakes/briefs-and-bug-reports.md).

## Web Dashboard Layout

Codex lanes were grouped all together instead of per forwarder/job pair, and a nested cluster frame narrowed its heat strips out of line with the plain lanes. Group on AgentFlow.forwarder, cancel a nested frame's border and padding, and measure strip x ranges in a real browser. → [Web Dashboard](mistakes/web-dashboard.md)

## Cargo test stops at the first failing target

A failing test binary stops cargo before it runs the rest, which can hide a `tests/maintainability.rs` baseline failure. See [maintainability-baseline](mistakes/maintainability-baseline.md).

## Planning Sandbox Delivery

A read-denied directory is a writable private tmpfs under srt, a skipped real-sandbox test leaves the sandbox unproven, `Edit(doc/*)` covers `doc/loom`, and an `.ignore` file does not make an ancestor search safe. → [Planning Sandbox Delivery](mistakes/planning-sandbox-delivery.md)

## Plan Chat Delivery

Review handlers briefed without lock, guard and subscriber order; codex and fixer units that passed their own checks and failed the full gate, the reviewers or the 400/50-line limits; web logic slips (turn ids repeat across sessions, hunk-relative diff parsing, both authoring refusals). → [Plan Server Delivery](mistakes/plan-server-delivery.md#review-loop-handlers-were-briefed-without-lock-guard-and-subscriber-facts-2026-10-29)

## Treating this repository's conventions as loom's product behaviour

**What happened:** While planning the planner redesign, a draft decision asked whether loom should ignore DONE- plans inside feature directories, citing this repository's own .gitignore as if it were loom's behaviour.
**Why:** loom is developed with loom, so this repository's doc/, .gitignore and plan habits are easy to mistake for what loom does in every project. Loom is a product used to build other projects.
**Prevention:** Before stating what loom does in a project, find the code that does it. In target projects loom only manages the .loom/work and .worktrees ignore entries (loom/src/commands/repair/workspace.rs); everything else in a project's .gitignore is the project's choice, and planner features must work whatever the project ignores.
**Fix:** PLAN-planner-ui-redesign.md moves the DONE-/REVIEW- pattern change to this repository's migration stage only.

## Planner UI Redesign Delivery

A path-convention move that left hardcoded homes in the crate, jsdom tests green over a broken browser (stale fixtures, nested routers, unserved fonts), an unowned "spec changes before running" table, subset gates that hid four failures until integration-verify, briefs missing facts only the code held, and orchestration slips. → [Moving a Path Convention Left Hardcoded Homes](mistakes/plan-server-delivery.md#moving-a-path-convention-left-hardcoded-homes-in-the-crate-2026-10-29) and the entries after it The repair plan's own slips (a hidden component's hooks, a conditional hook that passed lint, rewritten tests that lost guards, parallel stages merging into red tests, a resolver dead end, a Codex backend whose ids failed its own validator): → [Planner UI Repair](mistakes/planner-ui-repair.md) The redesign's own slips (offer ownership keyed on the page, page text never read against live state, a view derived from the draft, integrity disputes that claimed guards, an acceptance run that missed consumers, journey checks written before the layout settled, codex units reporting success over unchanged files, a server announcing before its stop handler): → [Planner Redesign](mistakes/planner-redesign.md)

## Astra Review Continuation

Parallel review repairs need explicit ownership, retained user data, and finding-specific verification.
See [review-continuation-verification](mistakes/review-continuation-verification.md) for coordination and test lessons, and [review-final-wave](mistakes/review-final-wave.md) for fixtures that could not fail, git internals assumed from memory, and capping open-ended review rounds.

## Planner Review Fixes

A `loom-kickoff` fence and a composite opening reached the transcript because no test replayed a captured kickoff; a sent item and its record arrived in the opposite order and doubled a row; a fix worker satisfied a text wiring check with a comment; a chat fixture asked for a final write before the turn finished. Prevention: replay real record shapes through the mapper and the transcript, read each wiring match, run a turn to Idle before a final write. → [Planner Review Fixes Mistakes](mistakes/planner-review-fixes.md)

## Claude Code Changed Its Record Format and Text Matches Broke Silently

Claude Code began wrapping a delivered prompt in `<pasted_content id="X">` tags in the transcript and the prompt hook; every match on the delivered text stopped matching with no failing test, because the fixtures carried the old bare format. Prevention: capture fixtures from real transcripts after a Claude Code upgrade, and test a match on Claude Code's output with both forms. → [Claude Code wrapped delivered prompts](mistakes/planner-ui-repair.md#claude-code-wrapped-delivered-prompts-and-every-text-match-broke-silently-2026-11-03)

## Planner Polish

A container-query component in an unsized flex item (footer meters stacked), a stop notice produced twice from one event, spine plan rows that lost their slug to badge words, layout and glyph fixes judged from a stubbed payload or one surface, and worker slips. → [Planner Polish Mistakes](mistakes/planner-polish.md) Planner feedback fixes: a route test that sent the id form the web client never sends, a background hand-back that reopened activity after the closing Stop, a bulk route holding the project read guard across saves, tests removed with a page, a test file over 400 lines, `loom check` backgrounded. → [Planner Review Fixes Mistakes](mistakes/planner-review-fixes.md#a-route-test-sent-the-id-form-the-web-client-never-sends)

## Plan Amendment Strips completed_commit From a Merged Stage

**What happened:** during PLAN-planner-polish, integration-verify's session amended merged stage chat-and-shell-visual's `wiring` twice (11:45Z, 11:46Z). `plan/amendment_fields.rs::sync_stage_from_definition` set `completed_commit = None` because the field changed, but left `merged: true`. When the adjudication verdict re-queued integration-verify at 11:49Z, `verify/transitions/state.rs::describe_dependency_block` returned `MissingCommit` for the dependency, and the stage sat queued for 68 minutes.
**Why:** clearing `completed_commit` forces re-verification of an in-flight stage, but a merged stage cannot re-verify. The amendment broke the merged-implies-commit invariant that the scheduler relies on.
**Prevention:** an amendment must never clear `completed_commit` on a stage with `merged` or `merge_assumed`; integration-verify legitimately amends earlier stages' wiring.
**Fix:** operator recreated `loom/chat-and-shell-visual` at its merged tip and ran `loom stage complete chat-and-shell-visual --force-unsafe --assume-merged`.

## Visual requests reduced to DOM tests

A visual request needs a concrete design brief and a screenshot check, not only DOM-presence tests. See [mistakes/planner-visual-feedback-shortfall](mistakes/planner-visual-feedback-shortfall.md).

## Planner follow-ups delivery

A live region mounted twice, a test constant that passed vitest and failed the type check, a classifier test with a name the generator never writes, a fix that broke another stage's wiring literal, a container query on its own container, a CSS reset that lost on specificity, a screenshot clone left in the worktree root, a frozen contract that matched a plan's own truth check, and a scaffold order rustfmt reversed; then, from the background and slash work, a `position: fixed` list inside a transformed ancestor that jsdom could not show, a browser driver never run before integration-verify, a fetch gated on an atom that test harnesses seed, a contract briefed as guarding what it could not, and workers running gates outside their territory. See [mistakes/planner-follow-ups-delivery](mistakes/planner-follow-ups-delivery.md).

## Planner design: stubs vs summaries, briefs unsummarized

Stubs (vision section paragraphs, metaplan entries) scope a document before it exists; summaries are generated after it is final, for review. A plan's summary covers its brief bundle, so a reviewer sees the briefs. See [planner-design-brainstorm](mistakes/planner-design-brainstorm.md). Delivery slips of the planning trace (settle hash source, socket test deadlines, CSS import order, contrast on the composited colour, assertion lines, wiring after a sibling merge) are in [planner-follow-ups-delivery](mistakes/planner-follow-ups-delivery.md#a-socket-test-waited-on-a-frame-that-never-came).

## dev-install.sh and the Running Daemon

Why `dev-install.sh` stops every loom process after the install, the review finding that removed the kill, and installing the operator's binary unasked. → [dev-install.sh and the Running Daemon](mistakes/dev-install.md)

## Planner spec scope and run review location

- A spec section adds an action, button or skill only when the vision names it; integration-verify's spec-conformance duty aligns spec and build. See [spec sections invented product features](mistakes/planner-design-brainstorm.md#spec-sections-invented-product-features-the-vision-never-asked-for).
- Feature run reviews land at the top of `doc/plans/`, and the readers search the plan's home, its `archive/` and the top of `doc/plans/`. See [a feature plan's run review](mistakes/planner-design-brainstorm.md#a-feature-plans-run-review-is-written-where-nothing-looks-for-it).
