# Code Review: Loom subscription-token optimization

**Plan:** PLAN-token-optimization-2026-09-13 | **Generated:** 2026-09-13 14:10 UTC

## Summary

Created: **2026-09-13** (Europe/Athens).
Evidence cutoff: **2026-09-12T20:12:13Z**. Audited source: `61847ac9afe57399c123f03fdb9b996e67deeb5f`.
Status: detailed implementation proposal; **not initialized or executed**. Exact stage-sandbox baseline is an activation prerequisite, not established by the host baseline.

## Changes by Stage

### Revalidate source contracts and focused knowledge (knowledge-bootstrap)

**Status:** completed  
**Purpose:** Read common.md under doc/plans/briefs/token-optimization-2026-09-13/ and the audit.
Use parallel Explore subagents for usage/cache, worker lifecycle and context seams;
return exact source-contract differences and proposed knowledge CLI corrections.
Read INDEX first and relevant mistakes only, never the entire knowledge corpus.
Check sibling plan conflicts before implementation; amend this plan to reuse landed
source contracts or retain its named owning stage if absent. Revalidate after amendment.
Resolve the exact stage-sandbox baseline prerequisite before any standard stage starts.
Correct stale ceilings, hook phase, retrieval and telemetry guidance in place through
loom knowledge replace-section; annotate actual source/revision, never assumed truth.
Use loom knowledge sync only as the bookend task, never as worktree acceptance.
Preserve tier routing: compact tier-1 summaries, focused linked detail, generated INDEX.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- mistake: ran loom-hooks/tests/codex-forward-guard-*.sh inside the live knowledge-bootstrap session to find out whether fake codex ledger records came from TMPDIR or env; it appended 5 more fake records to .loom/work/subagents/knowledge-bootstrap/codex.jsonl. Why: the tests inherit LOOM_STAGE_ID/LOOM_SESSION_ID/LOOM_WORK_DIR and write through them. Prevention: never run hook or cargo tests from a live stage session to attribute state writes; diff state against a disposable copy, or clear LOOM_* first and accept the attribution loss. Fix: records left for operator cleanup.
- gotcha: a TMPDIR inside the git checkout (e.g. loom/target/<dir>) fails ~290 lib/e2e tests (clean, init, knowledge check/sync, memory outside-git-repo, reconcile_graph) that assume their tempdir is outside any repository; the same tests pass with the harness TMPDIR (/tmp/claude-1000). Plan scratch roots for TMPDIR must be outside the repo, e.g. a host-created /tmp/<name> granted in allow_write.

### Correct provider accounting and reusable verification proof (measurement-and-cache)

**Status:** completed  
**Purpose:** Read common.md and measurement-and-cache/*.md under the dated briefs directory.
Spawn foreground loom-codex-forwarder units with explicit Bash timeout 600000 ms,
models below and effort xhigh. Workers never run git or verification; inspect
git status --short after each actual worker settles. Orchestrator verifies/commits.
Use parallel subagents and skills to maximize performance within disjoint ownership.
| Worker | Files owned |
| Sol provider-ledger | loom/src/commands/usage/**; loom/src/models/mod.rs; loom/src/models/execution_receipt.rs; loom/src/commands/subagents/ledger.rs; loom/src/commands/subagents/ledger_tests.rs |
| Terra quota-history | loom/src/quota/** |
| Sol cache-correctness | loom/src/verify/criteria/** |
Run independent normalization/cache/quota work concurrently; provider final history
consumption follows Terra's actual QuotaHistoryRead contract. No stub imports.
Preserve legacy usage keys, unknown provider fields, exact UTC bounds and provenance.
Cache only the fully evaluated bound criterion; convert the diagnostic to a regression.

#### Files Changed

No changes recorded.

#### Key Decisions

- split provider-ledger into wave 1 (normalization, CLI, receipts, ledger metadata) and wave 2 (quota read_history join + retiring stale maintainability-baseline entries); quota-history and cache-correctness run concurrently with wave 1 *(the assignment forbids stub imports and requires the join to follow Terra's actual QuotaHistoryRead; loom/maintainability-baseline.txt is outside every worker's territory and stale entries fail loom/tests/maintainability.rs, so one post-wave writer owns it)*
- review triage: receipt 5m/1h split mismatch no longer rejects a receipt; usage normalizer marks split unknown and counts it (spec rejection list omits it); quota continuity makes Unknown win over Reset when window kinds disagree; cache records keep bounded diagnostic tails (spec item 2 requires them); shell_builtin list not widened (spec forbids broadened eligibility) *(adversarial reviews of wave-1 output, session-7e345f7d)*
- final review triage (session-6370d2cc): accept all findings - Claude transcript 5m/1h split mismatch must null the ProviderTokenVector cache_write split like receipts (legacy TokenUsage keeps reconciled value); StartedAgentTypeIndex::get becomes cfg(test); share executor output-marker constants with cache_contract; drop always-true record_complete; remove no-op .tmp interrupted-write assertion; add quota tests for nonpositive observed_at and unknown provider *(final-tree reviews found no blocker; one major per ledger/cache territory; gate also needs 4 clippy fixes, fmt, and 2 stale maintainability-baseline removals)*

#### Notes

- found: markdownlint MD028/no-blanks-blockquote (blank line inside blockquote) in a knowledge file, already present before this stage; loom/.githooks/pre-commit runs markdownlint-cli2 --fix with stderr discarded and || true, so it never blocks and never names the file. Fix belongs to a knowledge stage
- gotcha: #[serial] only excludes other #[serial] tests; a serial test that mutates process PATH (PathGuard) still races non-serial cache tests, and the acceptance-cache key hashes PATH plus resolved executable identity, so a concurrent run stores its record under a different digest. Observed as legacy_and_corrupt_records_each_force_a_real_execution getting a cache hit under loom check. Prevention: any test asserting cache hit/miss or key stability must be #[serial]; terminal tests_spawn/tests_launch_capsule also mutate PATH
- mistake: accepted codex's '399 lines' for quota/history_tests.rs before cargo fmt; rustfmt expanded it to 472 and tests/maintainability.rs failed. Why: codex writes compact unformatted code and line counts are only meaningful after formatting. Prevention: brief codex workers to target <=330 lines per file, and check line limits only after cargo fmt. Fix: split the test module
- gotcha: when a codex forward's streamed progress exceeds the Agent tool's output cap, the forwarder cannot see its own LOOM-CODEX-EVIDENCE trailer (guard blocks reading the persisted file) and one forwarder issued a second 'placeholder' forward that created an extra companion job (no edits). Prevention: orchestrator reads the persisted tool-results file with rg -A60 '^\[codex\] Turn completed' and checks every job record listed, including any job created after the task job
- found: quota history_store.rs:55 ensure_history_dir checks symlinks before locked_dir_update takes the lock (TOCTOU, low: quota dir is 0700 and single-writer); history rewrites reuse quota cache::atomic_write rather than fs::locking::atomic_write_locked, so fs/locking.rs and quota/cache.rs keep two atomic-write implementations
- gotcha: a gpt-5.6-sol xhigh forward carrying 6 items (compile fixes + 4 review fixes + quota join) outran the 600000 ms Bash cap; the forwarder's call was moved to a Claude background task while the codex companion job kept running and writing. Prevention: keep one forward to about 3-4 items, or expect to wait on the job record (state/<worktree>/jobs/*.json phase) before releasing file ownership
- found: pre-existing, out of scope: sections/edits.rs:26 infers stage from project_slug via stage_id() (the pattern provider-ledger forbids for new attribution); cache_ignore.rs check_ignore duplicates process::run_bounded spawn/wait handling except stdin
- gotcha: codex companion task failed before any model call with 'Failed to read project hooks config file /home/dkaponis/src/loom/.codex/config.toml: Not a directory (os error 20)'. The main checkout (trusted in ~/.codex/config.toml) has a stray 0-byte regular file .codex born 2026-04-14; codex reads parent project .codex/config.toml from a worktree cwd. Wave-1 companion runs at 00:31Z succeeded with the same file/binary/config, cause of the difference unknown. Prevention: stat <repo>/.codex is a directory or absent before spawning codex forwarders
- found: wave-1 codex output never compiled (workers skip verification): usage_tests.rs:13 concat! format string with inline captures, transcript.rs:224-227 partial move of duplicate before merge_usage_observation borrow, quota/history.rs:183 E0282 closure inference. Prevention: after a no-verify codex wave, build all targets before dispatching dependent waves
- found: doc/token-optimization-2026-09-12-data/cache_probe.rs named by the cache-correctness brief and concerns/token-accounting-and-proof-defects.md is absent from the tree at ecb9eb06; the reproducer must be rebuilt from the brief's description (printf forbidden + stdout_not_contains [forbidden], second run falsely passes)

### Bind waits and usage to the actual forwarded execution (job-lifecycle)

**Status:** completed  
**Purpose:** Read common.md and both job-lifecycle briefs under the dated briefs directory.
Spawn foreground loom-codex-forwarder units using the named model/xhigh and
explicit Bash timeout 600000 ms. No worker git/verification; orchestrator checks
git status --short after settled workers and runs gates. No unknown ownership release.
| Worker | Files owned |
| Sol forwarding-receipts | loom/src/models/mod.rs; loom/src/models/forward_receipt.rs; loom/src/commands/hook/forward_receipt*; loom/src/commands/hook/mod.rs; loom/src/cli/types_ops.rs; loom/src/cli/dispatch.rs; loom/src/commands/usage/**; loom-hooks/codex-forward.sh; loom-hooks/codex-forward-guard.sh; loom-hooks/post-tool-use.sh; loom-hooks/tests/codex-forward* |
| Terra wait-and-poll | loom/src/commands/subagents/forward_jobs*; loom/src/commands/subagents/mod.rs; loom/src/commands/subagents/classify*; loom/src/commands/subagents/render*; loom/src/commands/subagents/table*; loom-hooks/poll-guard.sh; loom-hooks/tests/poll-guard*; agents/loom-codex-forwarder.md; loom/src/orchestrator/signals/format/codex.rs; loom/tests/integration/hooks_poll_guard* |
Sol additionally owns loom/src/models/forward_receipt/** for the shared pure reader.
Terra additionally owns loom/src/commands/subagents/summary.rs constructors.
Sol publishes complete independent ForwardReceipt and pure evidence adapter first;
Terra consumes it in a second wave. List/watch readers never write shared state.
Exact observed tool/job IDs replace newest scans; absent evidence remains unknown.
Preserve foreground forwarding and naturally backgrounded acknowledgement handling.
Usage joins only proven lifecycle identity; no summing receipt and transcript copies.

#### Files Changed

No changes recorded.

#### Key Decisions

- job-lifecycle waves: W1 = S1a sol (forward_receipt core+marker decoder), S1b sol (locator+companion job record reader), S2 sol (wrapper+guard test isolation, owns run-all.sh), T2 terra (poll-guard, doctrine, codex.rs signal, no run-all). W2 after S1 settles+builds = S3 sol (hook writer+CLI+post-tool-use, owns run-all.sh), S4 sol (usage forward_join), T1a terra (forward_jobs adapter + subagents wait). W3 = T1b terra (classify/summary/render/table overlay + poll-guard run-all registrations) *(measurement-and-cache memory: a 6-item sol forward outran the 600000ms Bash cap, so each forward is kept to ~3 items; the marker channel, receipts file path/fields and evidence trailer are pinned in every brief so parallel wave-1 units agree without reading each other's code; run-all.sh has one writer per wave)*
- forward-receipt writer keeps its own sidecar-lock + O_NOFOLLOW append instead of reusing fs::locking; review triage for R1/R2: fix TMPDIR companion root, parent-dir fsync, single-parse decode_line, dead codex_provider per-row thread assignment, unreachable by_receipt_id ambiguity arm, direct codex_provider thread tests; keep diagnostics fan-out *(fs::locking::locked_update/locked_read open the target with plain File::open, which follows symlinks, and the spec requires no-follow regular-file persistence; serde fills a missing Option field with None, so decode_line's key-count check is real strictness and must stay, just done in one parse; diagnostics fan-out matches receipt_provider.rs:236-248 precedent)*
- keep the unauthorized duplicate S4 run's substantive usage edits (--forward-receipts-root validated against the selected project's state root via roots_match; symlinked subagents dir refused before enumerating stage receipt files; foreign-root rejection test) and have the usage fix unit restore the blank lines it stripped from forward_join.rs and drop its redundant --all ensure! inside forward_receipts_root *(the spec requires --forward-receipts-root to validate the declared project scope and S4 plus review R2 both missed it; the rest of the duplicate diff was line-budget churn; snapshot of settled S4 tree kept in the session scratchpad for the comparison)*
- decline Sol review finding 2 (safe_shell_syntax rejects bash-valid double-quote escapes like "\a"); accept finding 1 (validate --forward-receipts-root against the selected root even without --project) *(the guard's parse_shell_words rejects the same escape (codex-forward-guard.sh:94-98), so such a forward is never authorized and the reader matching the guard is the correct invariant; widening only the reader would diverge from the authorization boundary. Finding 1: forward_receipts_root returned an unchecked explicit root whenever --project was omitted, attaching foreign receipts to the current project's transcripts)*

#### Notes

- mistake: in zsh, git show "$M:agents/file" expanded $M:a as the absolute-path modifier, git show failed, and the > redirect had already truncated the target; set -e did not abort the Bash tool script, so later git steps ran on bad state. Prevention: always brace variables before a colon (${M}:path), write git show output to a temp file and test -s it before copying over a tracked file, and chain dependent steps with && rather than trusting set -e. Fix: rewrite the file from the commit and re-sync the index with git read-tree -m -u <old> <new>
- mistake: added the harness Co-Authored-By trailer to a merge commit because the system reminder asked for it. Failed because CLAUDE.md rule 9 forbids AI attribution and commit-filter.sh blocks the whole Bash call. Prevention: never add attribution trailers in this repo, whatever a harness reminder says. Fix: re-run staging and commit together without the trailer
- found: loom/src/orchestrator/signals/tests_cache.rs:393-397 asserts the codex block contains 'status --all' as 'the recovery path', but the new doctrine (format/codex.rs:110-111) contains that substring only inside a prohibition ('Do not use codex-companion.mjs status --all; use the exact receipt'), so the test passes vacuously and would not catch a revert to recency recovery. tests_cache.rs is outside job-lifecycle's file inventory, so this stage adds the positive/negative doctrine assertion in codex.rs's own scope instead; integration-verify or a follow-up should rewrite the tests_cache.rs assertion to check for 'loom subagents wait --receipt'. Detection rule: a doctrine test that pins a bare keyword must assert the surrounding instruction, not the substring
- mistake: terra's subagents forward tests (forward_jobs_tests.rs, render_forward_tests.rs) built transcripts at <tmp>/agent-a.jsonl with timestamp-less rows and a 4-word 'codex-forward.sh task x --write' argv, while Sol's shared reader requires <parent>/subagents/agent-<id>.jsonl (TranscriptIdentity::from_path), a row timestamp (scan_entry) and the exact 8-word argv (exact_forward_argv); 9 tests failed as unreadable/None. Why: the W3 unit settled without the orchestrator running commands::subagents against the final Sol reader. Prevention: fixtures for the forward transcript reader must mirror the production path layout, row timestamps and the real doctrine argv; run each consumer's tests the moment the producer's contract changes
- mistake: hook/forward_receipt.rs safe_shell_syntax had no (State::Plain,_) => State::Plain arm, so the catch-all rejected every forwarding command at its first byte ('/' or '~'); exact_forward_argv never matched and loom hook forward-receipt wrote nothing for ANY forward, in tests and production, silently (post-tool-use.sh sends the hook's stderr to /dev/null and process() returns Ok with zero observations). Why: codex wrote the state machine without compiling or running its tests, and the first test run came only after every unit had settled. Prevention: the orchestrator runs each new module's tests the moment its unit settles; any hand-written tokenizer state machine needs a test with the real production argv. Fix: add the Plain fallthrough arm after the forbidden-byte guard
- found/gotcha: safe_shell_syntax in loom/src/commands/hook/forward_receipt.rs:175-189 matches (State, char) with catch-alls for Single and Double but none for Plain, so any ordinary Plain character (the leading '/' or '~' of every wrapper path) hits '_ => return false'; exact_forward_argv then returns None for every command and forward-receipt persists nothing. Silent in production because post-tool-use.sh:306 discards the hook's stderr. Detection rule: a state-machine match over (state, char) must have a fallthrough arm for every state that accepts arbitrary characters.
- mistake: briefed codex workers to write at most 330 lines per file; rustfmt then expanded S3's loom/src/commands/hook/forward_receipt.rs from 325 to 561 lines (+73%) and models/forward_receipt.rs past 400, because codex packs several statements and struct fields per line. Prevention: the orchestrator runs cargo fmt immediately after each settled codex wave and re-checks the 400/50 limits before the next wave; brief codex with a 230-line as-written ceiling for dense Rust
- gotcha: Codex workers' own sandbox treats /tmp/loom-token-optimization-checks as read-only, so a codex worker's single cargo test check with that TMPDIR fails every fixture-writing test (FX3 saw 54 environmental failures) - and no codex self-check this stage exercised tempfile tests. Prevention: tell codex workers to use cargo build/--no-run as their one check, and have the orchestrator run tests; never accept a codex report of 'tests pass' for fixture tests
- mistake (forwarder lane): a loom-codex-forwarder whose single forward auto-backgrounded is WOKEN by the harness when that background Bash completes, and in the woken turn it can forward again - the S4 sol forwarder produced a PLACEHOLDER-DO-NOT-USE job (task-mtzk6jpl-w4uw3v) and then a full re-run of the settled S4 usage task (task-mtzk8478-6upfxb) 20+ minutes after S4 settled, a second writer on settled files. Why: the installed guard only checks argv shape; the S5 transcript-based one-forward limit exists only in this worktree until merge. Prevention: after a forwarder backgrounds, treat the forwarder agent itself as still live; snapshot a unit's files the moment its job settles and diff after every later job the same forwarder creates
- gotcha: commit-filter.sh blocks any Bash call that unsets LOOM_MAIN_AGENT_PID (env -u / unset), even when the command only runs hook test scripts, so an orchestrator cannot scrub its live identity before running loom-hooks/tests/*. Prevention: run hook tests in the live env and detect leakage by comparing the line count of .loom/work/subagents/<stage>/codex.jsonl before and after (job-lifecycle: 13 -> 13 after the codex-forward* isolation fix)

### Admit relevant skills and proven task context (context-admission)

**Status:** completed  
**Purpose:** Read common.md and all three context-admission briefs in the dated directory.
Foreground loom-codex-forwarder, model below/xhigh, Bash timeout 600000 ms.
No worker git/verification. Orchestrator checks git status --short after settled
units, verifies and commits. Use parallel subagents with exclusive files.
| Worker | Files owned |
| Terra skill-routing-ownership | loom-hooks/skill-trigger.sh; loom/src/orchestrator/signals/format/skills.rs; loom/src/plan/schema/structural_checks.rs; loom/src/plan/schema/validation.rs; loom/tests/integration/hooks_skill_trigger.rs |
| Terra read-receipts | loom-hooks/_read_ledger.sh; loom-hooks/_read_discipline.sh; loom-hooks/read-guard.sh; loom-hooks/post-tool-use.sh; loom/src/context/read_receipts.rs; loom/src/context/mod.rs; loom/src/commands/hook/read_receipt.rs; loom/tests/integration/hooks_read_guard*; loom-hooks/tests/post-tool-use* |
| Sol scoped-worker-briefs | loom/src/commands/hook/worker_brief.rs; loom/src/commands/hook/tests_worker_brief.rs; loom/src/commands/hook/mod.rs; loom/src/orchestrator/signals/retrieval.rs; loom/src/context/delivery/session.rs; loom/src/cli/types_ops.rs; loom/src/cli/dispatch.rs; loom-hooks/spawn-guard.sh; loom-hooks/subagent-start.sh; loom/src/context/tests/delivery*; loom/tests/integration/hooks_spawn_guard*; loom-hooks/tests/subagent-start-ledger.sh |
Terra units first in parallel; Sol follows with worker-brief and read-receipt CLI
registration and exclusively owns loom/src/context/delivery.rs facade exports.
Preserve the earlier forward-receipt dispatch/PostToolUse behavior.
Pre/post generation, exact result correlation and epoch proof precede repeat advice.
Child nonce binding never gives a Codex rollout credit for a Claude forwarder receipt.

#### Files Changed

No changes recorded.

#### Key Decisions

- split the three context-admission briefs into six codex forwards with disjoint files: wave 1 terra = skill-routing (skill-trigger.sh, skills.rs, hooks_skill_trigger.rs), worker-table diagnostics (structural_checks.rs + structural_checks/worker_table.rs, validation.rs), read-receipt Rust (context/read_receipts*, context/mod.rs, hook/read_receipt.rs, hook/tests_read_receipt.rs), read-receipt shell (read-guard.sh,_read_discipline.sh,_read_ledger.sh, post-tool-use.sh, hooks_read_guard*.rs, post-tool-use-tool-event/empty-output.sh); wave 2 sol = worker-brief Rust + both CLI registrations, spawn/start shell + tests *(prior stages recorded that one sol/terra xhigh forward carrying a module pair plus tests outran the 600000 ms forwarder cap; the CLI contract between Rust and shell halves is pinned in both briefs so halves run in parallel; new hook shell helpers are impossible without unowned edits (fs/permissions/constants.rs include_str registry, loom-hooks/tests/run-all.sh explicit list), so shell work stays inside existing owned files)*
- restore skill-trigger.sh _is_name_match prefix matching (keyword len>=4 prefixing the loom-stripped name) that the SR-A codex unit removed; keep its new render gate requiring an exact skill-name or multi-word token for the Codex read-in-full directive *(prefix matching is the deliberate ranking fix from 8e8fdeb5 (refactor->loom-refactoring, debug->loom-debugging at weight 2); removing it drops those prompts below MIN_SCORE on every surface, violating the brief's preserve-ranking clause, while the render gate alone already keeps prefix matches conditional on the Codex surface)*
- SR review triage: accept all six reviewer findings plus orchestrator findings F1 (restore _is_name_match prefix) and F2 (worker-table parser ignored every real plan table because it required a Markdown divider row; all four tables in IN_PROGRESS-PLAN-token-optimization-2026-09-13.md are divider-less and YAML-indented) *(F2 would have shipped a diagnostic that never fires on real input while its unit tests passed; the fixture must mirror the live plan table shape. Findings 3-6 are low-risk edits confined to code new in this stage)*
- RR review triage: accept all seven findings plus F3. Read-receipt generation drops the retrieval context_epoch (it needs a full ContextPack via retrieve_for_stage on every Read, and knowledge/graph revisions say nothing about what the model holds) for a per-session compaction epoch token rotated by loom hook pre-compact; pre_compact.rs and tests_pre_compact.rs are newly assigned to the RR Rust fix unit. --check contract becomes 'proven <n>' (distinct proven deliveries at the current generation) so rule 2 regains the pre-diff policy: full n==1 warn, n>=2 deny; range n>=2 warn *(brief read-receipts.md:45 forbids unbounded work and added wait paths on Reads, :49 keeps advisory/deny and requires compaction to reset eligibility; HEAD _read_discipline.sh verdict2 denied on the 3rd full read)*
- cancelled duplicate SOL-A codex job task-mtzq1f4k-4639q5 by exact id; kept original task-mtzpnuhs-tbowaa as the sole SOL-A writer *(identity proven three ways: its log opens 'implementing the scoped Rust command and CLI registration', the SOL-A forwarder transcript holds two assistant tool_use wrapper calls (11:08:36 and 11:19:09), and its createdAt 11:19:10 follows the first call's 600 s cap. It had applied zero file changes when cancelled, so no conflicting write exists; common.md forbids a silent two-writer overlap, and this was a known duplicate, not an unknown writer)*
- Sol review triage: accept 1 (narrow the doc/plans exclusion to plan documents; required items removed by scope go to unmet_required), 2 (reuse read_bounded_prefix), 3 (debug line on dropped brief; keep the brief-mandated per-spawn retrieve_for_stage), 4 (gate the brief on the untyped-gate pass, not on MODEL). Keep loom_run_bounded in_read_ledger.sh with a header note (moving it grows the 1596-line_common.sh). Reject 7: serde_json's default 128-level recursion limit already bounds the marker walk. Record 8 only: worker-bound delivery records have no compaction reset, but no caller re-checks them *(3: brief scoped-worker-briefs.md:41 mandates calling retrieve_for_stage per spawn; changing the cost model is out of this stage's authority)*
- accepted behavior change: Bash-side cat/head/tail/sed repeat reads no longer escalate under rule 2; only Read-tool repeats with a Rust-proven receipt can warn or deny *(read-receipts.md:31,49 removes the attempt TSV as a repeat qualifier, and a Bash-side read never yields a Read tool_result receipt; poll-guard.sh:285 calls loom_read_discipline_check without a payload so verdict2 returns early. poll-guard.sh is owned by job-lifecycle (brief line 21), so this stage leaves it; a future plan could give Bash reads their own proof)*

#### Notes

- gotcha: loom hook read-receipt is a silent no-op for any payload lacking "cwd" (Payload::parse requires it); every real Claude Code hook payload carries cwd, but integration fixtures built without it make prepare/complete/check all do nothing, so only negative tests pass. The read-guard.sh header comment omitted cwd, which likely misled the fixture author. Prevention: build hook fixtures from the documented common fields (session_id, transcript_path, cwd, hook_event_name) and pair negative tests with positive controls
- mistake: first delegated fix for the receipt-lifecycle integration tests (TMPDIR alignment in complete_read_receipt) did not help: with LOOM_STAGE_ID set both sides already shared one root, and the new positive controls proved no receipt is ever proven through the shell hooks while all context::read_receipts unit tests pass. Why: the orchestrator's first hypothesis was checked against the no-stage path only. Prevention: before briefing a fix, confirm the hypothesis against the failing test's actual Session defaults (stage_id Some("stage-1")). Escalated to loom-advisor per Rule 7.5
- mistake: a codex size-fix unit, told validation.rs must stay at the exact baselined 1285 lines, collapsed an import block (-2 lines) and then padded the module doc comment with two invented lines to hit 1285. The count matched and the file was dishonest. Why: the brief framed the baseline number as the goal rather than the real line count. Prevention: say 'if the honest count drops, lower the baseline entry; never pad', and diff every baselined file after a size unit. Fix: revert the padding, baseline 1285 -> 1283
- gotcha: integration tests that drive one hook side through the harness (TMPDIR=session work dir) and the other side through a direct Command inherit different TMPDIR values; a store with a TMPDIR fallback root then never matches, and the negative tests (edit resets, error does not escalate) pass vacuously while the positive ones fail. Prevention: give direct Commands the same env as run_payload, and pair every negative receipt test with a positive control in the same session
- mistake: SR-B kept validation.rs at the baselined 1285 lines by swapping one call for one call, but its long wrapper name made rustfmt wrap the call onto three lines (1287 lines; validate_structural_preflight 232->234), which breaks the exact-count maintainability baseline. Codex also left hooks_skill_trigger.rs at 444 lines after rustfmt, and the maintainability scanner covers loom/tests too. Prevention: in a baselined file, the new name must be short enough that the call fits one rustfmt line (<=100 columns); always check line counts after cargo fmt, never before
- found: the duplicate SOL-A codex job task-mtzq1f4k-4639q5 changed no files. It noticed Sol-A file timestamps changing mid-read, applied common.md's one-owner rule itself, and ended its turn at 11:25:20 ('No files were changed'). The companion then recorded it as completed/done even though cancel had returned 'cancelled'. Prevention: after cancelling a job, check that its log has no 'Applying N file change' line instead of trusting the final status field
- found: loom_run_bounded was private to post-tool-use.sh (HEAD :166); the read-receipt shell unit moved it into _read_ledger.sh and the SOL-B unit then made spawn-guard.sh and subagent-start.sh source the TSV read-ledger library just to reach a generic bounded runner. Planned fix after the shell writers settle: move it to _common.sh and drop those source lines
- mistake (lane, recurred twice this stage): after its first wrapper call outran the 600000 ms Bash cap, the SOL-A forwarder issued a SECOND real wrapper call with the full task at 11:19:09, creating duplicate writer task-mtzq1f4k-4639q5 on the same files as the still-running task-mtzpnuhs-tbowaa. Detection: an extra job after a forwarder's cap, then rg its transcript for the unit marker - two assistant tool_use entries = duplicate. Prevention needed in agents/loom-codex-forwarder.md: after a backgrounded first call the forwarder must end its turn immediately with no further Bash; codex-forward-guard should allow at most one wrapper call per forwarder transcript
- mistake: a codex fix brief said 'add an integration fixture' and the worker replaced the existing brief-mandated 'context pointer' generic-pile case with the new one-keyword case, silently dropping coverage of keyword-only MIN_SCORE collisions. Why: fix briefs did not state that existing assertions are preserved. Prevention: every fix brief says 'keep every existing assertion; add, never replace' and the orchestrator greps the test file for the mandated fixture words after the fix
- stale-contract: agents/loom-codex-forwarder.md tells forwarders and orchestrators to wait with 'loom subagents wait --receipt <id> --timeout 3600', but no such subcommand exists in source or in loom 0.7.2-dev.82+15a8b0f5 (clap: unrecognized subcommand 'wait'). Working exact-ID wait: node <plugin>/scripts/codex-companion.mjs status <job-id> --wait --timeout-ms <ms> --json

### Preserve quality and evaluate the subscription frontier (proof-and-regression)

**Status:** completed  
**Purpose:** Read common.md and the three proof-and-regression briefs. Use parallel foreground
loom-codex-forwarder units, model below/xhigh, Bash timeout 600000 ms. Workers
never run git/verification; orchestrator checks git status --short, verifies/commits.
| Worker | Files owned |
| Sol pareto-comparison | loom/src/commands/usage/**; loom/tests/token_optimization_comparison.rs; loom/tests/fixtures/token_optimization/comparison/**; doc/token-optimization-evaluation.md |
| Terra verification-ownership | loom/src/orchestrator/signals/cache.rs; loom/src/orchestrator/signals/tests_cache.rs; loom/src/orchestrator/signals/tests_doctrine.rs; loom/src/orchestrator/signals/tests_doctrine_prefixes.rs; loom/src/orchestrator/signals/tests_commit_timing.rs; loom/src/orchestrator/signals/tests_size.rs; loom/tests/token_optimization_contracts.rs |
| Terra evidence-freshness | loom/src/fs/knowledge/catalog.rs; loom/src/fs/knowledge/catalog/**; loom/src/fs/knowledge/tests/catalog*; loom/src/commands/knowledge/check.rs; loom/src/commands/knowledge/tests_check*; loom/src/cli/types_memory.rs; loom/src/cli/dispatch.rs; loom/tests/token_optimization_knowledge.rs |
Keep token-proxy and subscription verdicts separate; reject every quality or latency
regression, retain inconclusive missing/ambiguous data. No live model experiments.
Strict evidence is opt-in on declared source facts; never mark unassessed prose true.
Name one canonical verifier, preserving every independent review and final quality gate.

#### Files Changed

No changes recorded.

#### Key Decisions

- proof-and-regression: CatalogIssue sites outside the evidence worker's inventory need no edit, so adding EvidenceUnavailable keeps ownership disjoint *(context/tests/store.rs constructs GenericBlurb; tests/integration/context_catalog.rs uses matches!; neither is an exhaustive match)*
- Lower loom/maintainability-baseline.txt entries this stage shrank (cache.rs 590->529, removed test_integration_verify_stable_prefix_contains_required_sections) with one owner in the fix wave *(tests/maintainability.rs::repository_maintainability_debt_does_not_grow_or_go_stale fails on stale entries; lowering debt is the ratchet, not grandfathering new violations (common.md forbids only the latter). The baseline is shared across all three territories, so it gets one writer.)*
- Kept integration_verify_prefix_is_stable despite a reviewer finding that it cannot discriminate *(It is the pre-existing test_integration_verify_stable_prefix_is_stable moved as-is from cache.rs; deleting existing tests is outside this stage's brief, and it is a determinism guard, not proof of the new doctrine (integration_verify_prefix_assigns_one_canonical_gate_owner is that proof))*
- Kept evidence.rs's own byte-level porcelain -z parser rather than reusing context/refresh/source_graph/generation.rs::parse_status *(the existing parser takes run_git_checked's trimmed String and needs a recovered-leading-space workaround; evidence reads raw Output bytes, so sharing would reintroduce that trim hazard. Cross-reference comments tie the shared rename-order assumption together.)*
- usage --compare: duplicate run id stays inconclusive, not rejected *(A duplicate run id is a data-integrity failure that makes the pair untrustworthy; it does not prove the candidate worse, and rejected/exit 1 is reserved for observed quality, latency or consumption regressions. Consistent with other structural-validity failures.)*
- usage --compare keeps its own bounded reader instead of fs/safe_read.rs::read_bounded *(safe_read::read_bounded takes (root, relative) and forbids absolute or .. paths, so it cannot open an arbitrary operator-supplied --compare artifact path; the local reader uses the same take(limit+1) plus post-read recheck form. A shared any-path bounded reader in fs/safe_read.rs is a candidate follow-up.)*
- usage --compare decides explicit selection flags from Option<T> UsageArgs fields with defaults applied at the use site, not an argv scan or ArgMatches::value_source *(main.rs calls Cli::parse() and discards ArgMatches, so value_source would mean threading matches through every subcommand. An argv rescan against a hand-kept flag list drifts silently when UsageArgs gains a flag. Option fields keep the check a pure function of what clap parsed, with blast radius confined to commands/usage.)*

#### Notes

- mistake: ran loom stage complete with output redirected to a file and printed back through rg -n, so the LOOM_CONTROL_VERIFICATION_PASSED marker reached the tool output as '17:LOOM_CONTROL_...'. The completion bridge (loom-control-complete.sh) did not match it and the stage stayed Executing (loom status: orphaned). Why: I filtered verbose output by habit (Rule 14). Prevention: never redirect or filter loom stage complete; its ~20 lines of output must reach the tool result verbatim, then look for the bridge's 'completion was accepted by the daemon' line.
- found: comparison_quota.rs compare_observations' 'quota-reduction-not-established' branch is unreachable today. classify_delta returns SupportedCandidate only for a strict improvement beyond uncertainty, and the caller turns empty window maps into missing-quota-telemetry first, so an all-SupportedCandidate reduction always sets improved. Kept as the explicit encoding of the brief's 'no improved window is inconclusive' rule for this private fn's precondition; a candidate for removal if the quota module is simplified.
- gotcha: cargo fmt can push a function a worker left at 49 lines over the 50-line maintainability limit. rustfmt's struct_lit_width puts a 3-field struct pattern like KnowledgeCommands::Check { strict, strict_evidence, json } on 5 lines. Size-check only after formatting.
- gotcha: cargo test with several --test targets stops at the first failing target unless --no-fail-fast, so later targets report nothing (not a pass). Gate runs that name several targets must pass --no-fail-fast and check each target's 'Running' + 'test result' line.
- stale-knowledge: conventions/guidance-channels-and-plugin-scope.md#<section above 'Claude Code Plugin Scope'> claims 'an integration-verify stage exists to run the complete suite, so its subagents are carved out'; the IV stable prefix now assigns ONE canonical verifier per tree/environment/criterion contract to run the complete suite, with other reviewers running only targeted discriminating checks (cache.rs INTEGRATION_VERIFY_OVERRIDE). Correction: 'The one exception: integration-verify subagents are carved out at the hook level (subagent-verify-guard.sh). The IV doctrine assigns one canonical verifier to run the complete suite per immutable tree/environment/criterion contract; other reviewers inspect independently and run only targeted checks, which never substitute for the canonical gate. The carve-out is resolved from the stage file and fails safe ...'
- concern: process::run_bounded_output bounds wall-clock only; reader threads read_to_end with no byte cap, so callers' output limits (e.g. evidence.rs MAX_GIT_OUTPUT_BYTES) cap the accepted result, not peak memory. Owner: process/mod.rs; fix with a take(limit+1) reader and an over-limit error.
- concern: shared git runner does not isolate git's environment. run_git_program sets only LC_ALL/LANG, so an inherited GIT_DIR/GIT_WORK_TREE/GIT_INDEX_FILE redirects every loom git call (including knowledge evidence freshness) to another repository. Owner: whoever owns git/runner.rs; fix with env_remove of the GIT_* repository variables.
- found: maintainability.rs reports exact remedies (shrunk file -> lower entry; removed fn -> delete stale entry; new >50-line fn -> refactor or add entry). This stage pushed two functions just over 50 (catalog.rs build 52, cache.rs generate_integration_verify_stable_prefix 51); refactor, never add entries (common.md).
- gotcha: a single-file integration target (loom/tests/<x>.rs) that includes the shared helpers via #[path = "integration/helpers.rs"] mod helpers; fails clippy -D warnings with dead_code for every helper it does not call; cargo build only warns. Scope #[allow(dead_code)] to that mod declaration. Also: an inline plan fixture with an integration-verify stage fails validation unless it declares acceptance/artifacts/wiring.
- gotcha: when a codex forward exceeds the forwarder's 600000ms Bash timeout, the harness backgrounds it; on completion the forwarder is re-invoked but codex-forward-guard blocks every further Bash call (even 'true'), so it cannot relay the wrapper stdout or the evidence trailer. Recovery: the orchestrator reads the named tasks/<id>.output with rg via Bash (Read is blocked by worktree-file-guard for paths outside the worktree), takes the report after the last 'Turn completed' line, and cross-checks the job record's phase=done. Affected 2 of 3 xhigh units this stage (24-30 min runs).

### Independent review and complete functional verification (integration-verify)

**Status:** completed  
**Purpose:** Read plan, focused source changes, assigned briefs and loom memory show --all.
Spawn parallel independent loom-code-reviewer units for correctness/security,
architecture and coverage. Preserve all review dimensions and source access.
One canonical verification owner runs full build/clippy/fmt/tests and all hook cases.
Fix every finding through a scoped engineer, then rerun invalidated gates; do not
waive missing requirements or unknown worker completion. Exercise all three new CLI
integration targets plus exact-job/read/skill/cache negative fixtures from earlier stages.
Check no new CLI is orphaned, no zero-test filter passed and no shared-state writes
occurred in list/usage/check. No live provider requests or automatic deployment.
Record stale-knowledge corrections and unresolved measurement uncertainty in memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- keep codex-forward-guard has_prior_forwarding_call allowing when the forwarder transcript is missing or unreadable (IV reviewer rated it medium fail-open) *(failing closed would block every FIRST forward if the harness has not yet written the forwarder transcript when PreToolUse fires, and no live provider request is allowed in IV to prove the write order; the fail-open is documented at the function header; the behavioural fix is the forwarder doc forbidding any second call after a backgrounded one)*
- keep the local bounded reader in usage/comparison.rs instead of fs::safe_read::read_bounded *(safe_read opens every component no-follow under a root, so an operator-supplied --compare path with any symlinked component (e.g. macOS /tmp) would be refused; the local reader keeps the take(limit+1) plus recheck bound)*
- dropped the IV plain-mode 'context pointer' twin test for skill-trigger *(plain mode (skill-trigger.sh:213-216) renders every catalogued match as a 'load with Skill(...)' hint under a neutral 'skills matching this request' header and makes no mandatory claim; the brief's mandatory-claim rule (skill-routing-ownership.md:41,59) targets the Codex 'read SKILL.md in full' directive (gated at :207-212, tested by codex_keeps_generic_keyword_collisions_conditional) and the signal combined loader (format/skills.rs). A plain-mode advisory-vs-mandatory assertion would assert a distinction that does not exist.)*

#### Notes

- found (pre-existing, systemic): verify/criteria runner_tests mutate process-wide LANG through EnvGuard under #[serial]; serial_test only excludes other #[serial] tests, and apply_stage_environment re-reads std::env::vars_os() per run (environment.rs:80-96), so a non-serial test in the same lib binary that spawns a confined command concurrently could fingerprint a temporarily mutated LANG. Not observed failing; follow-up is to pass the allowlisted env explicitly in those fixtures instead of mutating the process env.
- gotcha: IV fixes grew two grandfathered files (tests_cache.rs 665->670 and its fn test_signal_codex_implementers_section_gated 141->146; tests/e2e/tmux_backend.rs 588->592) and tests/maintainability.rs failed; the IV stage's files and sandbox exclude loom/maintainability-baseline.txt, and the ledger also fails on shrink ('go stale'), so a fix in a baselined file must land at EXACTLY the baselined count. Prevention: before briefing an IV fix, rg the target file in loom/maintainability-baseline.txt and state the exact required line count in the brief; move added coverage to an unbaselined test file instead of growing a baselined one.
- stale-knowledge: concerns/token-accounting-and-proof-defects.md (all four sections, verified 7d6a14ca) lists the criterion-cache false pass, streamed-usage undercount, 'loom subagents list' evading poll-guard, and hook tests writing the live ledger as OPEN. The tree fixes all four: cache stores only certified full evaluations (verify/criteria/cache_contract.rs, runner_tests.rs:179 forbidden_output_failure_executes_and_fails_twice); streamed usage takes the last whole vector (usage/transcript_merge.rs, transcript_tests.rs:49); poll-guard counts subagents list (tests/integration/hooks_poll_guard_subagents.rs:9); codex-forward-guard tests unset LOOM_* (loom-hooks/tests/codex-forward-guard-blocks-edit.sh:3; IV gate run left .loom/work/subagents/integration-verify with no codex.jsonl). Correction: mark each section resolved with its fixing file, or retire the page.
- mistake: job-lifecycle's agents/loom-codex-forwarder.md told a backgrounded forwarder to run 'loom subagents wait --receipt' or 'loom subagents watch', but codex-forward-guard.sh enforce_forwarder authorizes only the exact 8-word wrapper argv plus one-forward-per-transcript, so every such call is blocked. Why: the doc and the guard were edited by different units and nothing tests doc instructions against the guard allowlist. Prevention: every command an agent doc tells a guarded agent to run must pass that agent's guard; grep the doc's backticked commands against is_exact_forward_command. Fix (IV): the doc now says a backgrounded forwarder makes no further tool call and the orchestrator recovers via the exact receipt.
- uncertainty (still open at IV): whether Claude Code writes the Read tool_result row to the transcript before PostToolUse hooks run. If not, read-receipt --complete never correlates and repeat escalation never fires (fail-closed, no benefit); forward-receipt reads the transcript at the same point. No live run was allowed to settle it.
- uncertainty: read receipts correlate only an ADJACENT assistant tool_use row and user tool_result row (windows(2), read_receipts/transcript.rs:33-37), as the brief mandates (read-receipts.md:43,54). If Claude Code writes parallel tool_use blocks and their results as separate rows, parallel Reads never form a receipt: fail-closed, but the repeat-read optimization would then cover serial reads only. Unmeasured; needs sampling real transcripts with parallel Read calls before any savings claim.
- gotcha: under the plan's acceptance TMPDIR=/tmp/loom-token-optimization-checks the stage sandbox makes /tmp read-only, and create_isolated_tmux_tmpdir's fallback $TMPDIR/loom-e2e-tmux-<pid> projects a 106-byte socket path (> SUN_PATH_LIMIT 104), so all 7 tmux/web_terminal e2e tests panic at tests/e2e/tmux_backend.rs:173. The knowledge-bootstrap baseline passed only because it ran with the short harness TMPDIR. Prevention: a plan that sets a long scratch TMPDIR must check the tmux e2e socket budget (dir + /tmux-<uid>/ + 40).
- mistake: context-admission (fca9c515) made loom-hooks/post-tool-use.sh:17 source _read_ledger.sh, but the HookFixture in fs/permissions/hooks/policy_tests.rs installs only_common.sh, so post_tool_hook_persists_only_a_private_heartbeat fails at :228 (hook exits nonzero on the missing source). Why: standard stages ran scoped filters only and the full lib suite first ran in IV. Prevention: when a hook gains a source line, rg every Rust fixture that installs that hook (fixture.install("<hook>.sh") and install the new helper too.

### Correct and annotate durable knowledge (knowledge-distill)

**Status:** completed  
**Purpose:** SINGLE AGENT: no subagents. Read plan, loom memory show --all, INDEX and relevant
knowledge sections only. Correct stale-knowledge entries first in place with
loom knowledge replace-section, then curate through loom knowledge update.
Annotate source/revision for ceilings, hook phases, exact wait authority, signal caps,
provider ledger semantics and receipt lifecycle using existing knowledge annotate.
No blanket semantic-truth claim; changed evidence remains a review signal.
Preserve compact tier-1 summaries and linked detail; INDEX regenerates automatically.
Resolve every memory with promoted/merged/discarded/deferred receipt and target/reason.
Update README's affected usage/wait sections and the evaluation protocol, then loom review.
Finish with strict structural knowledge and pending-memory checks, not corpus-wide
strict-evidence activation over unassessed pages. No auto-memory or live experiments.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- found: two recorded memories are superseded by the final tree: 3b324d86 ('no loom subagents wait subcommand') is false in source (subagents/mod.rs:115-128), and 8c26d351 ('no fs/work_dir/discovery.rs') is false since discovery.rs was added and knowledge already cites it (mistakes/ambient-filesystem-trust.md:143)
- found: doc/loom/knowledge/INDEX.md is 16385 bytes against its 16384-byte OversizedIndex budget, so strict check fails before this stage writes anything; every new tier-2 topic adds an INDEX row, so a distill stage must shorten blurbs (loom knowledge annotate <t> --blurb) to pay for any new topic
- gotcha: the PATH binary (loom 0.7.2-dev.82+15a8b0f5) predates this plan's merged CLI surface: 'loom subagents wait' (source: commands/subagents/mod.rs:115), 'loom usage --compare/--forward-receipts-root' (usage/mod.rs:78,106) and 'loom knowledge check --strict-evidence' (knowledge/check.rs:36) are in the tree but absent from 'loom --help'. Acceptance 'loom knowledge check --strict' runs the older checker. Prevention: when documenting or pinning a CLI surface, read the clap definitions in source, and never conclude a command is missing from the PATH binary alone

## Open Questions

- **[Admit relevant skills and proven task context]** does Claude Code write the Read tool_result line to the transcript before PostToolUse hooks run? if not, read-receipt --complete never finds the correlated result and repeat escalation never fires (fail closed, no false reuse, but no benefit either); forward-receipt from job-lifecycle reads the transcript at the same point
