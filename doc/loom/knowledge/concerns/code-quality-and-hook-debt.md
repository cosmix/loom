# Code Quality And Hook Debt

> Debt

## Oversized Rust Units Remain Controlled Debt

The maintainability gate does not yet prove that every production Rust file is at most 400 lines or
every function at most 50 lines. Existing violations are recorded by exact identity and size in
`loom/maintainability-baseline.txt`; `loom/tests/maintainability.rs` rejects new entries, growth of an
existing entry, or a stale baseline entry. CI runs that gate, so the exception set can only stay flat
or shrink. Treat this as controlled decomposition debt, not as completed decomposition, and remove
an entry whenever its unit is brought under the limit.

## Hooks Ship Only Through the Embedded-Assets Binary

Remote releases deliver hooks through the embedded-assets binary. `install.sh::main` downloads or builds the binary and then invokes `loom install-assets --skills "$SKILLS_MODE"`. There is no `install_hooks_remote` function or separate per-hook release download. Hook inventory comes from `LOOM_HOOKS` in `loom/src/fs/permissions/constants.rs`, consumed by the asset installer.

## No-Verify Doctrine Block Carries Only a Rust Example

The shared no-verify doctrine is pinned across signals, `loom-hooks/_subagent-preamble.txt`, and the guard refusal. It carries one scoped-command example, using `cargo`; a blocked Python or Go subagent therefore sees a Rust example. Language-specific examples can be appended as hook-local guidance outside the pinned block.

## Subagent verification keeps a private shell scanner

`subagent-verify-guard.sh` strips embedded content, normalizes separators and walks command-position tokens with quote tracking; its runner-specific matchers live in the sourced `_verify_matchers.sh`. The private whitespace splitter is not a complete shell parser. Review quoted separators, wrappers, substitutions and scoped-runner options with differential adversarial fixtures before replacing it. The stage-type exemption greps the entire stage Markdown, so it should use frontmatter-only state. Shared token helpers alone do not establish shell equivalence.

## Hook responsibility boundaries and file sizes

Several shell hooks exceed the repository file-size limit. Refactor by responsibility while preserving shell sourcing order and hook registration.

- `loom-hooks/credential-guard.sh` 599 lines
- `loom-hooks/commit-filter.sh` 463 lines
- `loom-hooks/codex-forward.sh` 412 lines
- `loom-hooks/prefer-modern-tools.sh` 402 lines

`loom/src/fs/permissions/constants.rs` embeds hook sources and lists `LOOM_HOOKS`; the hook configuration builder registers invoked hooks, and asset tests check installed inventory. `install.sh` invokes the binary asset installer and has no duplicated `all_hooks` arrays. Add a new embedded helper to the inventory, and register it as an event hook only if it is invoked directly.

## Low-Severity Cleanups Deferred From the Verification Gate

- `delivery::plan_key` falls back to a `default` namespace when a stage file lacks `plan_id`,
  while `commands/context/record_edit::active_plan` and `MergeLifecycle::plan_id` both read
  `config.plan_id()`. They agree today, but a legacy stage file without `plan_id` would file
  delivery records under a different namespace than the graph overlay and dirty paths. Route
  all three through one derivation.

## Context Telemetry Delivery Evidence

`orchestrator/core/stage_telemetry.rs::record_context_telemetry` uses the strict delivery
observer to distinguish an absent record from an unreadable or corrupt ledger. The ordinary
retrieval loader remains tolerant; the observer treats any corrupt ledger row as unavailable.

## `git/worktree/settings.rs` Is Over the File Cap

The file is 462 lines, over the 400-line guidance, and is a recorded violation in
`loom/maintainability-baseline.txt`. It is multi-purpose: worktree `.loom/work`/`.claude`/`CLAUDE.md`
scaffold planting, settings generation and permission merging, and the git-exclude writer
(`add_to_gitignore_exclude`, `add_worktree_exclude_patterns`, `add_settings_local_to_worktree_gitignore`),
which is self-contained and has its own tests. A structural split is not a surgical change and
deserves a dedicated stage; any file at or near its ledger cap must be refactored in the same change
that grows it (see `mistakes/testing-and-lint.md`).

## Ledger TUI: Tech Debt From the Live-Ledger-Dashboard Plan

- **`StageSummary.session_backend` has no Rust reader.** Populated by `build_stage_summary`
  (`status/data/collector.rs`, from `session.backend`), but no ledger column, static
  renderer line, or attention entry reads it; every other Rust occurrence is a test-fixture struct
  literal. Its only reader is the web stage page (`web/src/routes/stage.tsx`), which shows the
  `loom run --backend tmux` command when the backend is `native`. Either give the ledger a BACKEND
  cell or accept the web as the sole consumer.
- **Apparently unused:** `pub use theme::{StatusColors,
Theme}` (`commands/status/ui/mod.rs`, every consumer imports `ui::theme::Theme` directly) and `pub use
app::TuiApp` (`commands/status/ui/tui/mod.rs`, used only by `run_tui` in the same file).
- **The stage-id path-component check exists in several independent forms.** Rust
  `status/data/sanitize.rs::valid_stage_id` (used by `execution_models_for_stage`) rejects `.`, any `..`
  and characters outside `[A-Za-z0-9._-]`; `commands/memory/handlers/work_dir.rs::validate_stage_id` rejects only
  path separators and `..` (a lone `.` passes); the shell writers use a leading-alphanumeric regex
  (`loom-hooks/codex-forward.sh`, `codex-forward-guard.sh`, `_read_discipline.sh`) or an explicit `.`/`..` case
  (`loom-hooks/spawn-guard.sh::record_spawn`) — see mistakes.md for the divergence history. Candidate for one
  shared helper.
- **Model-name display normalization (strip `claude-` prefix, strip trailing `-YYYYMMDD`) is
  implemented twice**: `commands/subagents/table.rs::display_model`/`strip_date_suffix` and
  `status/data/execution_models.rs::normalize_model`. The status/ copy was written
  independently because `commands/subagents/table.rs` sits outside the payload-parity stage's declared files. Worth
  collapsing into one shared helper in a stage that owns both paths.
- **`commands/status/ui/tui/app.rs` is below the 400-line file cap.** Its live daemon reader keeps partial frames
  across idle polls in `daemon/wire.rs`; focused split-frame tests live in `daemon/wire_response_frame_tests.rs`.
  `TuiApp::reconnect_after_read_error` cannot get a
  narrow unit test without mocking a live Unix socket plus a real
  `Terminal<CrosstermBackend<Stdout>>` — `TuiApp` owns both directly with no trait seam — so it is
  covered only by the full build.
- **The ledger footer's error branch has zero test coverage.** `TuiApp.last_error` is wired end to end
  from daemon exit / `Response::Error` through to `panels::render_footer`, but no ledger test ever
  sets it non-`None` (`commands/status/ui/tui/ledger/tests.rs::fixture`, `commands/status/ui/tui/ledger/layout.rs::notice_below_minimum` both pass `None`).

## Memory and Hook Follow-Ups After the Doctrine-Truth Plan

Reviewer suggestions the plan left unimplemented:

- `commit-guard.sh` (`remind_memory_usage`) and `learning-validator.sh` (`check_memory_usage`) can disagree for one stage: the first guards the stage id only against empty and `/` and counts a journal of more than 10 lines as entries, the second uses a character whitelist and looks for a `###` entry. Both are advisory and read-only, and the 10-line threshold equals at least one entry only while the journal header (`fs/memory/storage.rs::journal_header`) stays 8 or more lines. Align the guards if either becomes blocking.
- The post-commit box (`remind_memory_capture` in `post-tool-use.sh`) opens with "Record what this work taught you" and only its last line tells a knowledge-distill stage otherwise. A stage-type check would make the box say one thing per stage, if the hook can obtain the stage type.
- `fs/memory/parser.rs::parse_journal` keeps a `## Summary` branch that no code writes. The no-backward-compatibility rule argues for deleting it.
- `orchestrator/signals/tests_doctrine_commands.rs` scans the signal templates and skills but not `README.md`, `agents/*.md` or `loom-hooks/_subagent-preamble.txt`, which also teach loom commands.
- `fs/plan_lifecycle.rs::archive_run_state_before_done` repeats `archive_state_dir`'s two-line plan-id lookup because it already holds a `WorkDir` and an explicit main root; take a plan-id helper from `fs/memory/archive.rs` if a third caller appears. `archive_state_dir` has no direct test for its derived plan id and main root: the clean and init tests only check that an archive appears, so an archive named `default-<ts>` would pass them.
- `v2_lints/bookends.rs` imports `StageDefinition` and `StageType` on separate lines from `crate::plan::schema`; one `use crate::plan::schema::{StageDefinition, StageType};` matches the test file.

## Execution Graph And Forward Guard Gaps

> recovery sync resets a finished node; macOS forward-guard branches unrun

## Execution Graph: Recovery Sync Resets a Finished Node

`ExecutionGraph::mark_queued` (`loom/src/plan/graph/mod.rs::mark_queued`) refuses a node whose own
status cannot move to `Queued` (`Completed`, `Skipped`, `WaitingForInput`, `MergeConflict`);
`Executing` is allowed for the handoff requeue. `project_stage`
(`loom/src/orchestrator/core/recovery_sync.rs::project_stage`) treats any `mark_queued` error as
`force_status(WaitingForDeps)`, so a stale graph node that is `Completed` or `Skipped` while its
stage file says `Queued` is still reset and later re-promoted by `refresh_ready_status`. The stage
file is authoritative there by design; if the stale-graph case matters, skip the force when the
node is already terminal.

## Codex Forward Guard: macOS Stage-Evidence Branches Were Never Executed

`loom_stage_evidence` (`loom-hooks/_codex_forward.sh`) decides whether `codex-forward-guard.sh` enforces anything. Its Linux branches are covered by `loom-hooks/tests/codex-forward-guard-stage-evidence.sh` with real ancestry and a real `bwrap`. Its macOS branches were written and reviewed on Linux and have NOT been run. Background: [The forward guard engages only inside a loom stage](../architecture/codex-plugin.md).

**To do on a Mac, before relying on the guard there:**

- Run `bash loom-hooks/tests/codex-forward-guard-stage-evidence.sh` and `bash loom-hooks/tests/run-all.sh`. The Seatbelt case (guard under `/usr/bin/sandbox-exec -p '(version 1)(allow default)'` with a clean env, expects exit 2 naming `sandbox confinement (seatbelt)`) only runs on Darwin; on Linux it prints `SKIP`.
- E2 parent walk: `_loom_parent_pid` parses `/bin/ps -o ppid= -p <pid>`. Confirm the value is a bare integer after the `read -r` trim under bash 3.2.
- E2 environment read: `_loom_pid_env_has_stage_var` matches `LOOM_STAGE_ID=`, `LOOM_SESSION_ID=`, `LOOM_WORK_DIR=` in `/bin/ps eww -o command= -p <pid>`. Confirm `ps eww` prints the environment of the user's own `claude`/node process on the macOS versions in use (SIP and hardened-runtime processes can hide it), and that the nested-session test (an ancestor with `LOOM_STAGE_ID`, guard run under `env -i`) exits 2.
- `/bin/ps` is setuid root and may be refused inside a Seatbelt profile. If it is, E2 yields nothing there and E3 must carry the case: check that a guard run inside the stage sandbox with a scrubbed env still exits 2 through the `sandbox-exec` refusal probe.
- E3 probe: confirm `/usr/bin/sandbox-exec` and `/usr/bin/true` exist and that the probe SUCCEEDS (no evidence) in an ordinary unsandboxed session, so the stock Codex plugin is usable there: a `codex:codex-rescue` call in a plain session must not be blocked.
- The Rust test `forward_guard_allows_when_no_stage_evidence_exists` (`loom/src/fs/permissions/hooks/policy_tests_stage_gate.rs`) skips only on LOOM__in its own env or `/proc/1/comm == bwrap`. Under a macOS Seatbelt with no LOOM__ it would FAIL instead of skipping; add a Seatbelt skip if that combination occurs in practice.

**Known limits, all platforms:** a nested `claude` whose launcher controls its environment can set `BASH_ENV` or a config directory with no hooks, which no hook can police (sandbox policy's job); a plain session inside the user's own bubblewrap or Seatbelt wrapper counts as confined and stays blocked.

## Pinned Git Gitlink Route Is Unverified

The route is closed in the runner and has its fixtures: `without_gitlink_recursion` (`git/runner.rs`) adds `--ignore-submodules=dirty` behind every global option form, so a pinned or unpinned `git status` or `git diff` starts nothing inside a gitlink (`git/runner/tests.rs`, `git/runner/global_option_tests.rs`). `git stash create` and `git stash push` have no such option, so no loom code path runs them. The cost: uncommitted content inside a gitlink no longer shows in `status`, `diff` or the completion check; a moved gitlink commit still shows.

## Observer Source Uses Path-Only Locate

- `loom/src/verify/review/observer.rs::source` `source()` calls `locate(worktree)` on the given path, not `enclosing_stage`; a subdirectory of a stage worktree would get `WorktreeGit::discovered`. Every current caller passes the worktree root. Open, low.

## Redundant Canonicalize In Observer

- `loom/src/verify/review/observer.rs::checkout_git` canonicalizes again in `enclosing_stage`, and `locate` canonicalizes each ancestor. Correct, redundant syscalls. Open, low.

## Wait Request State After Writeln Failure

- `loom/src/host_verify/wait.rs::request`: a failed `requested` write after `write_state` leaves the request state, so a rerun follows a ticket never relayed until `REQUEST_GRACE` ends. Needs closed stdout to trigger. Open, low.

## Hook Rejects Commands That Name Stage Completion

- `loom-hooks/loom-control-complete.sh` (PreToolUse) rejects any Bash command whose text contains a stage-complete-like phrase, such as a Python heredoc that mentions "complete" near "request" or "stage". Put the edit script in a scratch file with the Write tool, or use the Edit tool.
- A subagent Bash command that names `src/commands/stage/complete_relay.rs` is refused as an unpinned `stage complete` call. Read such files with the Read tool.
- `subagent-verify-guard.sh` lets a subagent compile (`cargo check`, `cargo build`, `cargo test --no-run`) but still refuses `cargo clippy`, `cargo fmt` and an unfiltered `cargo test`, even when the brief asks for them. Briefs may name targeted tests for the code and module without a run-count limit; full suites run through the stage gate.

## Guidance-and-Gate-Output Plan Follow-Ups

Reviewer suggestions and findings the plan left unimplemented, with no fix in the plan.

**Hooks**

- **The large-output notice may never reach the model.** `_loom_bash_output_notice` in `loom-hooks/post-tool-use.sh` prints its notice on stderr and exits 0. Claude Code passes a hook's stderr to the model only on exit 2, so the notice reaches only the transcript. The fix shape is the stdout `additionalContext` JSON channel that `_loom_main_edit_advisory` uses ([hook-system.md](../architecture/hook-system.md)). The notice text already matches Rule 14 (log with `2>&1; echo "exit=$?"`, read failures with `rg -n`, no `tail`), and `loom-hooks/tests/post-tool-use-large-output-notice.sh` pins that wording.
- **`post-tool-use.sh` is 398 of 400 lines (Rule 17).** The next addition moves `_loom_main_edit_advisory` into a sourced helper, as `_read_ledger.sh` was split for size; a new embedded helper costs the four hook-registration surfaces (`fs/permissions/constants.rs`, the hook configuration builder, the asset tests, `install.sh`).
- **The advisory's tally is heavier than it needs to be.** It runs three `jq` processes per edit call (agent id, file path, line count), computes the ledger path twice and re-extracts the edited path that the EDIT RECORDING block already extracts. One `jq` call and a shared ledger-path helper would cut the hot-path cost; the recording block needs the `loom` binary and the tally must not, so the extractor cannot simply be shared.
- **Worktree containment is a lexical prefix test.** A symlinked spelling of the worktree never tallies, unlike `is_within` in `worktree-file-guard.sh`, which canonicalizes with `pwd -P`. Risk is low because `terminal/native/wrapper.rs` canonicalizes `LOOM_WORKTREE_PATH`.
- **The `warned` check and the `warned` append are not locked.** Two parallel main-agent edit calls that both cross the limit can each print the advisory once. Rare duplicate, no data loss.
- **`completion_rejection_reason` in `loom-hooks/loom-control-complete.sh` tests the unquoted-heredoc pattern first and against the raw command.** A refused command with an inner `cat <<X` inside a quoted body, or a `<<n` shift, reports "quote the delimiter" instead of the real cause (a separator, a relative path, a shell wrapper). Message text only; the refusal is unchanged. Check it after the `ATTEMPT_*` reasons.
- **`loom-hooks/session-start.sh` opens its re-anchor with "Context was compacted."** even for source `resume`, which template Rule 3b now routes to the same message; the wording could name both.
- **The Rule 14 recipe logs to `${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/<step>.log`.** With neither variable set that is a predictable name in shared `/tmp` (symlink clobbering on a multi-user host), and Rule 10 forbids reading outside the worktree while the Read tool cannot read scratch ([completion-runner-blocks.md](../mistakes/completion-runner-blocks.md)), so Bash `rg` is the only reader. The template sits at its byte ceiling, so a "scratch excepted" clause needs an offsetting trim. `skills/loom-debugging/SKILL.md` still shows a `for ...; do cargo test ... 2>&1 | tee /tmp/run || break; done` loop whose `|| break` tests `tee`.
- **A subagent `git rm --cached` has now happened twice** ([subagent-orchestration.md](../mistakes/subagent-orchestration.md)). A PreToolUse rule in `commit-filter.sh` or `git-add-guard.sh` that denies `git rm --cached` and `git restore --staged` for a subagent would replace the brief sentence.

**Doctrine text**

- **Review dimension 3 (`signals/cache/blocks.rs`) hands a reviewer "`loom check` output plus ALL stderr"** while the preamble forbids reviewers from running the gate; nothing tells the orchestrator to pass the check summary and the full-log paths to that reviewer.
- **`ORCHESTRATOR_GATE` says "for each failed criterion"**, which reads as one run per criterion; the repeatable `--criterion <n>` survives only in `README.md` and the CLI help (`commands/verify.rs`). "One run with a repeated `--criterion <n>`" would be exact.
- **The knowledge (bootstrap) stable prefix never carried BLOCK-F**, and template Rule 13 lacks "Filing ends this session" and "Commit before filing". Decide whether bootstrap sessions carry `STAGE_EXIT_RULES`.
- **BLOCK-F's "Commit before filing" and `append_commit_timing_rules` disagree**; see [agent-rule-bending-hardening.md](agent-rule-bending-hardening.md).

**Tests**

- **`tests_guidance_manifest.rs` has gaps.** `RELOCATIONS` has no length assertion while `FROZEN_LINES` is pinned at 40 lines and 21 headings, so deleting an entry together with its home text passes silently. Old template Rule 3's "committing nothing unverified (Rule 4)" has no entry. `the_writing_style_heading_is_still_mandatory` repeats `a_deleted_frozen_heading_fails_the_manifest_check` for a different heading and could fold into one table-driven test.
- **`CLAUDE_MD_TEMPLATE` and `ORCHESTRATION_SKILL` `include_str!` constants are declared in seven test files.** A shared test-constants module would remove it.
- **Go skill triggers.** `skills/loom-golang/SKILL.md` keeps the two-word triggers `go code`, `go test`, `go mod` and `in go` as weight-2 solo qualifiers; an imperative-"go" Rust prompt could hit them. `error` and `sync` are dropped by the indexer's `STOPWORDS` (`skill_index.rs`). `hooks_skill_trigger_evidence.rs` skips silently when `python3` is missing (`skip_unless_python3`); run the gate with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` so a skip fails. The Go test fails only when two or more removed bare words return, since one bare hit scores 1 against `MIN_SCORE` 2 (`skill-trigger.sh`); asserting that the built `skill-keywords.json` has no bare key for the ten removed words would catch a single one.
- **`loom/tests/guidance_gate_contracts.rs` spawns about 320 bash plus `jq` pipelines serially.** It is isolated and correct, and the slowest test of the suite for its size.

## The Binary Spawn Guard Runs Only in integration-verify

`tests/integration/binary_spawn_guard.rs` fails a test target that spawns the loom binary without scrubbing every `LOOM_*` variable or without joining `SANCTIONED_SPAWNERS`, but no stage gate runs it before integration-verify, so two consecutive plans (checklist-check's contract, plan-server-core's `plan_server_cli.rs` and `plan_server_contracts.rs`, the redesign's owner-watch test `plan_owner_watch_session.rs`) found the failure last ([Plan-Server Delivery](../mistakes/plan-server-delivery.md)). Proposal: have `loom plan verify` warn on a stage whose `files` list a `loom/tests/*.rs` that uses `CARGO_BIN_EXE_loom` and whose acceptance lacks `cargo test --test integration binary_spawn_guard`.

## Proposal: Run the Maintainability Test for Any File Within a Few Lines of a Size Limit

A worker whose one narrow check is a lib filter never runs `tests/maintainability.rs`, so a function grown past 50 lines or a file past 400 is caught only by the stage's full gate; it happened twice ([contract-autonomy-delivery](../mistakes/contract-autonomy-delivery.md#a-workers-one-scoped-check-missed-its-consumers-and-the-maintainability-test), [briefs-and-bug-reports](../mistakes/briefs-and-bug-reports.md#a-workers-compile-signal-never-existed)). Proposal, not built: a PostToolUse or `loom check` pre-step that compares the edited file and function lengths with `loom/maintainability-baseline.txt` and prints the offending function while the worker can still fix it; until then a brief for a function near the limit names `cargo test --manifest-path loom/Cargo.toml --test maintainability` as the worker's check.

## Hook False-Positive Plan Follow-Ups

State after the hook-guards work; the guard behaviour itself is in [Hook System](../architecture/hook-system.md#what-each-changed-guard-decides).

- **`commit-filter.sh` exceeds the 400-line cap.** Its 463 lines cover attribution, subagent git and the scratch exemption. `loom/tests/maintainability.rs` walks only Rust files, so it does not enforce the shell-hook cap.
- **Blocks the ledger does not record.** `commit-guard.sh`, `credential-guard.sh`, `spawn-guard.sh`, `plans-path-guard.sh`, `stage-terminal-guard.sh`, `codex-forward-guard.sh` and `poll-guard.sh`'s `loom_hook_deny_or_warn` never call `loom_record_block`, so `loom hook blocks` undercounts them. Codex-lane hooks cannot write the ledger at all: their sandbox denies `<repo>/.loom` and `loom_record_block` returns early on `LOOM_CODEX_FORWARD`. Infrastructure exits are also unrecorded: `worktree-file-guard.sh:45` (policy-load and `pwd` failures) writes no line while `worktree-isolation.sh`'s `block_policy_load` records `policy-load`, an inconsistency with the rule that infrastructure exits stay out. A comparison with the transcript evidence needs a transcript re-mine, not the ledger. The ledger records `role: main` for an unknown verdict; recording `unknown` would keep the by-rule main/subagent counts exact. A subject can still carry Unicode bidi format characters (U+202A-202E, U+2066-2069), which `char::is_control` does not replace in `blocks.rs`.
- **A `.git` gitfile in a scratch repository retargets plain git** just as in-worktree git shares refs. `loom_git_targets_scratch_only` reads repository state at hook time and rejects tokens naming a `.git` component, but a script or git hook the command runs can still reach elsewhere. The exemption is best-effort like the rest of `commit-filter.sh`; the capsule stays the write boundary. A redirected git segment (`git commit -qm x 2>&1 | tail -3`) never gets the exemption, so a subagent hits a block there; `LOOM_TOKEN_LITERAL_DOLLAR` is command-wide, so one single-quoted `$` (`-m 'cost $5'`) disables scratch expansion for the whole command. Both err toward blocking.
- **Resolver and tokenizer limits (fail open or over-restrictive).**
  - A backslash-newline yields a newline token that `loom_tokens_command_word_index` reports as the command word, hiding `git` from `loom_policy_git_tokens` and `loom_git_runs_commit` (`cd x && \<nl> git -C /elsewhere commit`); a segment starting with a redirection is likewise invisible.
  - `eval`/`source`/`.` count only at a command word, so `function f { . ./g; }; f` can reassign `LOOM_SCRATCH_DIR` for worktree-isolation's classing; treating `function` and `coproc` as unsafe command words closes that spelling.
  - The resolver resolves `..` physically where bash `cd` is logical: `cd .loom/work/..` is tracked as `<repo>/.loom`, bash lands in `<wt>/.loom`. Over-restrictive.
  - `~` expands to the hook's `HOME`; a quoted `'~'` or an earlier `HOME=` assignment differs from bash. It fails closed unless `HOME` lies in the worktree or scratch. An inherited `CDPATH` is not seen (only a `CDPATH` token in the command is).
  - The git global-option scan in `_worktree_policy.sh` and `_git-command.sh` stops at an unlisted option that takes a separate value (older git's `--super-prefix`), hiding a following `-C`; list it or fail closed on an unlisted `--long` option without `=`.
  - The tokenizer drops quotes, so `git -C '$LOOM_SCRATCH_DIR'` reads as the variable; the real target is `<cwd>/$LOOM_SCRATCH_DIR`, so there is no new exposure. Revisit if the tokenizer ever reports quote kinds.
  - `own_re` in `check_cross_worktree_tokens` is unanchored, so `<repo>/.worktrees/other/.worktrees/<stage>/f` matches the own-stage exemption.
  - `traversal_kind` and `traversal_path_part` run in command substitutions per token: hundreds of forks on a long command, costly on macOS; return the kind through a global.
- **File-guard residuals.** `allow_tmpdir` refuses another session's uuid directory only at depth 2; sibling stages' temp files directly under `/tmp/claude-<uid>` stay readable to the Read tool. `allow_tmpdir` and `allow_tool_results` leave a gap between the hook's check and the harness read (a symlink swapped in by a background process); Bash already reads the target, so no capability is added, but a comment should name it.
- **`subagent-verify-guard.sh` gaps.** `tsc --noEmit false` is read as tsc with file argument `false` and passes although tsc emits the whole project; only `tsc --noEmit false -p .` is pinned as blocked.
- **`spawn-guard.sh` header (lines 37-39) is stale.** It says Claude Code discards an `updatedInput` that carries no `permissionDecision`; Claude Code 2.1.319 no longer does.
- **Test and portability gaps.**
  - No gate exercises bash 3.2: the `LOOM_HOOK_TEST_BSD=1` run shims only `wc`, `stat` and `date`, not bash 3.2, `/private` symlinks, BSD `ls -ldn`, `env -u`, `mktemp`, `sed` or `iconv`, and `check-hook-syntax.sh` uses the host `bash -n`. Whether bash 3.2 honours `local LC_ALL=C` in `_loom_utf8_cut` is unverified; if not the cut is 300 characters (up to 1,200 bytes), still under the reader's 4,096-byte limit.
  - `loom-hooks/loom-control-complete.sh`'s process-walk branch of `completion_caller_is_main` (unknown verdict, no `transcript_path`, `! loom_is_subagent`) is untested; a fake live claude chain with a payload stripped of `agent_type` and `transcript_path` would pin it.
  - `loom/src/commands/hook/tests_blocks.rs`'s FIFO test would hang until the suite timeout, not fail, if `O_NONBLOCK` regressed; bound it with a thread join. `loom/tests/read_gate_contracts.rs::control_inputs_are_exempt` runs the exemption only against the warn path (`Gate::live`); running it under `Gate::outlined` would pin the deny path.
  - `loom-hooks/tests/_fixture_base.sh` leaves an empty `/tmp/claude-<uid>/loom-fixtures/<uuid>` directory after each run.
  - `loom/tests/integration/hooks_subagent_verify_guard.rs` panics at `temp_dir_no_claude` when `TMPDIR` contains `claude`, as the Claude Code sandbox's does; run it with `TMPDIR` set to a claude-free writable directory (`$LOOM_SCRATCH_DIR`).

## Hook Tests That Rebuild PATH Keep Leaking `LOOM_HOOK_PATH`

`LOOM_HOOK_PATH` leaked into a hook test a second time: `loom-hooks/tests/require-jq-advisory.sh` printed `FAIL: expected exit 1 when jq is missing, got 0` from a live stage shell, because `_read_discipline.sh` restores `PATH="${LOOM_HOOK_PATH:-$PATH}"` over the test's `path_without` PATH (it passed in the host job, where the variable is unset). Each new test must remember its own `unset`. Proposal: `loom-hooks/tests/_path_without.sh` unsets `LOOM_HOOK_PATH` itself, or `run-all.sh` runs every test under `env -u LOOM_HOOK_PATH -u LOOM_WORK_DIR -u LOOM_SESSION_ID -u LOOM_STAGE_ID`, so no test can forget. The mistake is in [Hook Tests Inherit the Live Session's Environment](../mistakes/hooks-shell-portability.md#hook-tests-inherit-the-live-sessions-environment-2026-09-19).

## Rule 6 Coordinators Contradict the Spawn Guard

`skills/loom-orchestration/SKILL.md` Rule 6 says a coordinator is spawned as `general-purpose` with a
`model` override, because the engineer agent types carry no Task tool. The spawn guard
(`loom-hooks/spawn-guard.sh:223`) denies every `general-purpose` spawn inside a gated stage session,
and the same skill says inside a stage untyped spawns are denied. A stage main agent therefore cannot
spawn a 2-level coordinator as written. Decide in a follow-up plan between allowing a typed
coordinator agent (with the Task tool) and removing the 2-level hierarchy from stage sessions.

## Budget and Routing Followups

- **Rounding.** `budget.rs` prints `{:.0}` (half to even) while `under_pressure` compares the raw
  percent, so 74.6% shows "75% used" with no routing line. Rounding once and comparing the rounded
  value flips the unfrozen `budget_tests.rs:48` case (74.9, 1801 s, false); decide whether the 75%
  threshold applies to the raw or the displayed value first.
- **Future readings.** `fresh()` treats an `observed_at` in the future as fresh indefinitely
  (negative age); `quota/cache.rs` `sanitize` does not clamp it. Only constant routing text is
  affected. Treat `observed_at` beyond now plus a skew as stale.
- **No omission test.** Knowledge and contract signals read the quota cache for a `model_budget`
  they never render (`signals/knowledge.rs:33`), and no test pins that they omit `## Model Budget`.
- **`first_listed()`** (`models/stage/implementers.rs:70`) has only test callers; remove it or mark
  it test-only. `implementer_tests.rs:36,60-64` still names "preference order" in a test name and
  comments; rename to listed-order wording.

## Usage Attribution Limits

- `discovery.rs::filter_stages` matches `ends_with("-worktrees-<stage>")`, so `--stage b` also
  selects the slug of stage `a-b`; reuse `stage_from_project_slug` for one rule.
- With `--all` or no readable work dir `ParentSpawns` holds no stage set, so `.worktrees/foo/loom`
  yields stage `foo-loom`, marked known (`parent_spawns.rs:52`). Leave the stage null when
  `work_dir` is `None`, or document the limit at `:31-35`; revisit if `--all` gains per-repo stage
  lists.
- A transcript from a session started in a worktree subdirectory has slug
  `<repo>--worktrees-<stage>-loom`; the recorded-stage check turns it into no stage. If Claude Code
  maps `_` to `-` in slugs (unverified), `a_b` parses as `a-b`.
- The 10-byte-needle byte-window `contains` in `parent_spawns.rs:125` and `git/hooks.rs:214` is the
  same idiom; share a helper only if a third caller appears.

## Spawn Guard and Hook Fixture Edges

- A non-object `tool_input` makes the `. + {model}` merge in `spawn-guard.sh:235` fail; the hook
  exits non-zero and fails open without the sonnet rewrite. Claude Code always sends an object.
- `loom-hooks/tests/_fixture_base.sh:25`: `mkdir -p` in the `/tmp/claude-<uid>` branch has no
  explicit failure check, and inside a command substitution `errexit` may not propagate. Add
  `|| return 1`.
- The prefer-modern-tools hook warns after a `grep` inside `bash -c` has run. A blocking check on
  the wrapper form would stop the recurrence recorded in
  [review-integration-verification](../mistakes/review-integration-verification.md).

## Other Open Observations

- Adjudication prompt builders re-inline the `## Dispute` header: `push_dispute_header`
  (`orchestrator/adjudication/prompt/criterion.rs:310`) exists, but `findings.rs:64`, `impact.rs:66`,
  `integrity.rs:61`, `contract.rs:252` and `freeze_refusal_evidence.rs:62` do not call it.
- The planner spine's Findings well counts `decided = findings.length - openCount`
  (`web/src/plan/feature/spine-instruments.ts:79`), so unverified, stale and duplicate open
  findings read as decided once `openCount` counts only actionable ones.
- The design record for the chat background line (`ui/DESIGN.md`, Chat) was not in this tree to
  check against the widened `waitsOnBackground` condition; compare it when the file is next edited.

## The Retry Silence Report Mirrors `MAX_STALL_RECOVERIES`

`commands/stage/skip_retry_silence.rs` keeps its own `MAX_STALL_RECOVERIES = 2`, because the constant is private to `orchestrator/core/event_handler/recover_hung.rs`, and it derives the escalation threshold by probing `never_worked::is_escalation` since `STALL_ESCALATION_MULTIPLIER` is private to `orchestrator/monitor/stall_line.rs`. A change to either value leaves the retry report wrong without a failing test. Fix: make both `pub(crate)` in `stall_line.rs`, import them in `recover_hung.rs` and `skip_retry_silence.rs`, and delete the local copy.

## Relay Guard Tokenizer Edges

Two accepted edges of `loom-hooks/loom-relay-guard.sh`, both from the shared tokenizer (`loom-hooks/_parser.sh`):

- **Quoted descriptor digit.** `loom check s --criterion '2'>log` redirects stdout in bash (a quoted `2` is not a descriptor), but the glued flag in `LOOM_TOKEN_META` reads it as fd 2, so the guard passes it. Fix: mark quote-derived words as non-descriptors.
- **Comments are not modelled.** An unmatched `(` inside a `#` comment (`# (draft` on one line, `loom check s` on the next) raises the next command's depth to 1, and the guard falsely refuses it as captured. Modelling `#` in the tokenizer changes every hook that sources `_parser.sh`, so it needs its own change with the full hook suite.

## Tests Shell Out to Perl and Python

Rust tests reach other languages for helper programs: 6 files under `loom/src` run `perl`, three by absolute path (`/usr/bin/perl` in `verify/criteria/confine_seatbelt_tests.rs`, `confine_seatbelt_loopback_tests.rs` and `confine_stage_cache_tests.rs`) and three by bare name (`confine_host_tests.rs`, `confine_seatbelt_signal_tests.rs` and `escapees_tests.rs`), and 80 `.rs` files mention `python`/`python3`, beside the Python scripts under `scripts/` and the bash hooks. Each language adds an interpreter the test host and the sandbox must provide (macOS `/usr/bin/python3` is an xcrun shim). A probe that needs a real process inside a boundary can re-run the test binary itself instead (`std::env::current_exe()` with a hidden mode), as `process/host_job_lane_tests.rs` and `pressure/record/publication_tests.rs` do. Open: migrate the remaining Perl and Python helpers to it.

## Hook Proposals From Recurring Orchestrator Mistakes

Two mistakes recurred across plans, so they get a mechanical guard proposal instead of another paragraph.

- **Editing the tree while `loom-code-reviewer` runs** ([first record](../mistakes/planner-follow-ups-delivery.md#editing-a-file-while-the-code-reviewer-ran-skipped-its-round), again in `open-github-issues`: four reviewers each ended as `review_harvest_failed.worktree_changed`, no round was recorded, and the findings had to be carried into the fix brief by hand). The review hook binds a round to the tree fingerprint at spawn. Proposal: a PreToolUse guard that refuses `Edit` and `Write` in the main session while a `loom-code-reviewer` it spawned has not returned, naming the outstanding reviewer; `loom check` failures are fixed before spawning.
- **A Bash command whose text holds both `stage` and `complete` is refused as an unpinned completion** ([first record](../mistakes/codex-worker-briefing.md#verification-loops-hid-the-tool-they-called), again twice in `open-github-issues`: a Python heredoc mentioning `completed_stages`, an inline script mentioning `stageCounts(record).completed`, and an edit script about plan completion). The text scan in `loom-control-complete.sh` cannot tell prose from a command. Proposal: scan only the command's first word and arguments for the exact `loom stage complete` shape, never heredoc bodies or quoted script text. Until then write edit scripts with the Write tool to the scratch directory and run `python3 <file>`.

## Efficiency-Plan Debt: Hook File Size, Unwired False Positives, Baseline Pressure

- **`loom-hooks/commit-filter.sh` is 463 lines**.
  The repository's 400-line file cap is not enforced for `loom-hooks/*.sh`: `loom/tests/maintainability.rs`'s scanner walks
  only the loom crate's `build`, `src` and `tests` Rust files, so no gate sees hook scripts. A refactor stage should
  either split the script or extend the scanner to hooks.
- **The stage-completion unwired-file detector uses the resolved source graph.**
  `verify/wiring_detection.rs:37-115` inspects resolved edges, marks missing or incomplete graph
  evidence uncertain, and excludes conventional test paths. `#[path]` support depends on graph
  extraction and resolution; any remaining false positive needs a current fixture with its graph
  evidence. It is not a grep of importable names in `mod.rs`.
- **Every new `Stage` field costs a baseline line.** `Stage::default` (`models/stage/defaults.rs`) is the single
  exhaustive literal and its `loom/maintainability-baseline.txt` entry (`function src/models/stage/defaults.rs default 76`)
  is exact; the next field needs a field-grouping refactor (nested structs) rather than another raise.
- **Two masked-exit shapes are still misjudged** (`plan/schema/validation/criterion_hazards/masked_exit.rs`): `cargo build && (cargo test ||
true)` is flagged although a failing build still fails it, and `cmd || true 2>/dev/null` is not flagged. Both are rare;
  they are recorded in [plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md).

## Non-Serial Tests Race Process-Wide State Their Serial Siblings Rewrite

`serial_test` orders a `#[serial]` test only against other `#[serial]` tests. `verify::criteria::confine::host::seatbelt::provision_tests::provisioning_denies_the_stage_roots_and_the_scratch_base` failed once under the full suite because `provision_network_rules` reads `HOME` (`confine_seatbelt.rs`) and the test reads it again (`confine_provision_tests.rs`) while serial tests repoint it (`test_cache_env.rs`, `scratch_home.rs`); marking the reader `#[serial_test::serial]` fixed it. This is the third instance of the pattern in [test-concurrency-and-fixtures](../mistakes/test-concurrency-and-fixtures.md#a-non-serial-test-read-an-env-var-a-serial-sibling-mutates-2026-09-03). Proposal: a `loom plan verify` or lint check that lists every test reading `HOME`, `PATH` or `LOOM_HOME` outside `#[serial]` in a binary where a `#[serial]` test sets it, so the fix is not found one flaky run at a time.

## Fixture-Replay Test Helper Leftovers

Reviewer suggestions on `plan_session/bridge/fixture_replay.rs` and its tests, left open because none is a defect: `consumed()` at `tests_background.rs:34` is redundant after the `wait_until` on both tasks (harmless, kept for symmetry); `consumed()` proves spool application only, so a test asserting transcript-derived state right after it (for example the agent steps at `tests_background.rs:57`) can race; the per-`received_at` count match over-counts for two same-millisecond lines that differ only outside the payload. Revisit when a test asserts on transcript state straight after `consumed()`.
