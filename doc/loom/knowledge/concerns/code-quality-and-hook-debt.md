# Code Quality And Hook Debt

> Oversized units, hooks

## Oversized Rust Units Remain Controlled Debt

The maintainability gate does not yet prove that every production Rust file is at most 400 lines or
every function at most 50 lines. Existing violations are recorded by exact identity and size in
`loom/maintainability-baseline.txt`; `loom/tests/maintainability.rs` rejects new entries, growth of an
existing entry, or a stale baseline entry. CI runs that gate, so the exception set can only stay flat
or shrink. Treat this as controlled decomposition debt, not as completed decomposition, and remove
an entry whenever its unit is brought under the limit.

## Debug Output in Production

`eprintln!` statements with 'Debug:' prefix in production code (complete.rs, orchestrator.rs). Should use tracing crate with log levels.

## Hook Debug Logging to /tmp/

Several hooks (worktree-isolation.sh, commit-filter.sh, prefer-modern-tools.sh) hardcode debug log paths to `/tmp/<name>-debug.log`. Under `set -euo pipefail`, if `/tmp/` is not writable (e.g., sandboxed environments), the hook script exits immediately with error. `git-add-guard.sh` already uses a gated `debug()` pattern that only writes when `GIT_ADD_GUARD_DEBUG=1` is set. Other hooks should adopt the same pattern.

## Rust/Shell Heredoc Terminator Divergence

The Rust `strip_embedded_content` in `bash.rs:79` uses `line.trim() == marker` (tolerates indented terminators), while the shell version in `_common.sh:44` uses `$0 == marker` (exact line match). Both fail-safe but should be aligned for consistency.

## Remote Releases Do Not Deliver Hooks (PRE-EXISTING)

Remote releases deliver hooks through the embedded-assets binary. `install.sh` downloads or builds the binary, checks that it exposes `install-assets`, then invokes `loom install-assets --skills "$SKILLS_MODE"` (`install.sh:354-366`). There is no `install_hooks_remote` function or separate per-hook release download. Hook inventory comes from `LOOM_HOOKS` in `loom/src/fs/permissions/constants.rs`, consumed by the asset installer.

## No-Verify Doctrine Block Carries Only a Rust Example

The doctrine block must stay byte-identical across the signal, the template, and the hook's
refusal message, so it carries a single scoped-command example — a `cargo` one. A blocked Python
or Go subagent is shown a Rust example. `loom-hooks/subagent-verify-guard.sh` is at the 400-line cap
with no slack, so the fix is to append language-specific examples **after** the pinned block as
explicitly hook-local guidance, the same way the `BLOCKED:` framing line already sits outside it.

## `subagent-verify-guard.sh` Still Regexes Raw Command Strings

It is the last hook that matches patterns against the raw command string, so it still cannot tell
an argument's _value_ from an argument's _mention_: text quoted inside a command is scanned as if
it were shell. The shared `loom_tokens_*` helpers it needs already exist in `loom-hooks/_common.sh`.

It was left for last because its failure direction is the mild one — it blocks project-wide
build/test/lint runs by subagents, so a false positive strands a subagent rather than admitting a
dangerous command. That is a reason to do it last, not a reason it is safe.

**Converting the fifth hook is not a mechanical edit.** The 2026-08-26 conversion of three hooks
opened seven bypasses that the raw regexes had blocked, all found only by adversarial probing
against the OLD pattern — a fully green suite showed nothing. Read
`mistakes/shell-command-matchers.md` § "Converting a Raw-String Matcher to Token Scanning
Silently Narrows It" before starting, and budget for the differential testing it describes.

## Three Hook Files Exceed Rule 17's 400-Line Cap

Several shell hooks exceed the repository file-size limit. Refactor by responsibility while preserving shell sourcing order and hook registration.

`loom/src/fs/permissions/constants.rs` embeds hook sources and lists `LOOM_HOOKS`; the hook configuration builder registers invoked hooks, and asset tests check installed inventory. `install.sh` invokes the binary asset installer and has no duplicated `all_hooks` arrays. Add a new embedded helper to the inventory, and register it as an event hook only if it is invoked directly.

## Duplicated Manifest-to-Language Tables

File extensions have one table: `context/extract/dialect.rs::DIALECTS`. `language.rs::language_for_path`
and `context/extract/lexical.rs::language_for_path` both derive from `dialect_for_path`, and
`language/tests.rs` asserts every `DIALECTS` extension maps to a `DetectedLanguage`.

Manifest files still have two tables that nothing pins together:

- `language.rs::detect_project_languages` feeds `DetectedLanguage` (the orchestrator's fallback skill
  recommendations and the `loom init` sandbox-domain suggestions).
- `skills/project/markers.rs::FILE_MARKERS` plus its probes decide project kinds for `loom project detect`,
  runner selection and package skills.

They disagree today: markers counts a `package.json` with a `typescript` dependency as TypeScript while
`language.rs` needs `tsconfig.json`; markers maps `build.gradle.kts` to `kotlin` while `language.rs` maps it to
Java; `language.rs` takes `meson.build`, `conanfile.*` and `vcpkg.json` as C++ while markers takes only
`CMakeLists.txt`; markers has kinds (`kotlin`, `scala`, `swift`, `elixir`, `dart`) with no `DetectedLanguage`
variant.

**Failure mode:** a project gets one language from `loom project detect` and another from the stage signal's
fallback recommendations. **Fix:** derive `detect_project_languages` from the markers probe, or move both onto
one manifest table.

## Low-Severity Cleanups Deferred From the Verification Gate

- `refresh/source_graph.rs:111` is a bare `let _ = mark_semantic_stale(..)` while that
  function's own doc (`:341-343`) says callers log and continue. The sibling caller in
  `merge_lifecycle` does log; this one does not.
- `delivery::plan_key` falls back to a `default` namespace when a stage file lacks `plan_id`,
  while `commands/context/record_edit::active_plan` and `MergeLifecycle::plan_id` both read
  `config.plan_id()`. They agree today, but a legacy stage file without `plan_id` would file
  delivery records under a different namespace than the graph overlay and dirty paths. Route
  all three through one derivation.

## Telemetry Under-Reports the Failures It Exists to Measure

`orchestrator/core/stage_telemetry.rs:25` calls `.ok()` on
`load_deliveries`, collapsing a genuine I/O error into the same
`ContextUnavailable { reason: "no delivery record for this session" }` as a real
miss. The file exists to measure how often stages spawn without a context brief, so
folding read failures into "no record" under-reports precisely the failures it is
for. **Fix:** give the error branch its own reason string.

## `git/worktree/settings.rs` Is Still Over Cap After Its Tests Moved Out (size reconfirmed)

Splitting the inline test module into `tests_settings.rs` / `tests_settings_env.rs` took the file
from 1263 to 637 lines and its ledger entry from 1119 to 637 — a large win, but the production
half is still well over the 400-line guidance and remains a recorded violation in
`maintainability-baseline.txt`. It has since grown further, to 706 lines
(`maintainability-baseline.txt:26`).

It is genuinely multi-purpose: worktree `.loom/work`/`.claude`/`CLAUDE.md` scaffold planting, settings
generation and permission merging, env scrubbing, and the git-exclude writer. Those are separable
— the exclude writer in particular (`add_to_gitignore_exclude`,
`add_worktree_exclude_patterns`, and the two `add_settings_local_to_*_gitignore` entry points)
is self-contained and has its own tests.

Not done as part of the sandbox bug fixes because a structural split is not a surgical change and
would have collided with four agents working the same tree. Worth a dedicated stage; note that
any file at or near its ledger cap must be refactored in the same change that grows it (see
`mistakes/testing-and-lint.md`).

## Ledger TUI: Tech Debt From the Live-Ledger-Dashboard Plan

- **`StageSummary.session_backend` has no Rust reader.** Populated by `build_stage_summary`
  (`commands/status/data/collector.rs`, from `session.backend`), but no ledger column, static
  renderer line, or attention entry reads it; every other Rust occurrence is a test-fixture struct
  literal. Its only reader is the web stage page (`web/src/routes/stage.tsx`), which shows the
  `loom run --backend tmux` command when the backend is `native`. Either give the ledger a BACKEND
  cell or accept the web as the sole consumer.
- **`render_context_bar` (`commands/status/render/progress.rs:56`, re-exported at `commands/status/render/mod.rs:18`) is dead**, with
  no caller anywhere in the tree — `commands/status/ui/tui/ledger/cells.rs:100-116` independently reimplements the
  same five-cell bar with the same characters. Also apparently unused: `pub use theme::{StatusColors,
  Theme}` (`commands/status/ui/mod.rs:4`, every consumer imports `ui::theme::Theme` directly) and `pub use
  app::TuiApp` (`commands/status/ui/tui/mod.rs:25`, used only by `run_tui` in the same file).
- **The `.`/`..` stage-id path-component check exists in four places at three different strengths**
  (`commands/status/data/execution_models.rs:38`, `commands/memory/handlers/work_dir.rs:99`, `loom-hooks/codex-forward.sh:43`,
  `loom-hooks/spawn-guard.sh:309`) — see mistakes.md for the resulting divergence. Candidate for one shared
  helper.
- **Model-name display normalization (strip `claude-` prefix, strip trailing `-YYYYMMDD`) is
  implemented twice**: `commands/subagents/table.rs::display_model`/`strip_date_suffix` and
  `commands/status/data/execution_models.rs::normalize_model`. The status/ copy was written
  independently because `commands/subagents/table.rs` sits outside the payload-parity stage's declared files. Worth
  collapsing into one shared helper in a stage that owns both paths.
- **`commands/status/ui/tui/app.rs` is pinned at exactly the 400-line file cap** after this plan's fixes (doc comment,
  reconnect retry loop + constants). Any future addition must trim an equal number of lines elsewhere
  or move content into `commands/status/ui/tui/app_tests.rs`. Relatedly, `TuiApp::reconnect_after_read_error` cannot get a
  narrow unit test without mocking a live Unix socket plus a real
  `Terminal<CrosstermBackend<Stdout>>` — `TuiApp` owns both directly with no trait seam — so it is
  covered only by the full build.
- **The ledger footer's error branch has zero test coverage.** `TuiApp.last_error` is wired end to end
  from daemon exit / `Response::Error` through to `panels::render_footer`, but no ledger test ever
  sets it non-`None` (`commands/status/ui/tui/ledger/tests.rs:189`, `commands/status/ui/tui/ledger/layout.rs:295` both pass `None`).

## Efficiency-Plan Debt: Hook File Size, Unwired False Positives, Baseline Pressure, Criterion Rendering

- **`loom-hooks/commit-filter.sh` is 494 lines** (491 before the hook-guards stage added one attribution sentence).
  Rule 17's 400-line file cap is not enforced for `loom-hooks/*.sh`: `loom/tests/maintainability.rs`'s scanner walks
  only the loom crate's `build`, `src` and `tests` Rust files, so no gate sees hook scripts. A refactor stage should
  either split the script or extend the scanner to hooks.
- **The stage-completion unwired-file detector does not follow `#[path]` test modules.** It flagged
  `commands/knowledge/tests_check_baseline.rs` as unwired although `check.rs:327` compiles it through
  `#[path = "tests_check_baseline.rs"] mod`; `signals/generate_declared_skills_tests.rs` and
  `generate_missing_allow_write_tests.rs` are wired the same way from `generate.rs:338-343`. The detector
  (`verify/wiring_detection.rs`) greps for the file's importable name, so a `#[path]` include declared outside a
  `mod.rs` reads as unreferenced. Teach it the attribute or accept the false positive knowingly.
- **Every new `Stage` field costs a baseline line.** `Stage::default` (`models/stage/defaults.rs:6`) is the single
  exhaustive literal and its maintainability entry was raised from 70 to 71 by decision; the next field needs a
  field-grouping refactor (nested structs) rather than another raise.
- **The stage signal renders an acceptance criterion as its bare command** (`signals/format/sections.rs:485`), dropping
  `exit_code`, so a criterion that must FAIL (`exit_code: 1`) reads as a positive check and misled two worker briefs.
  Render the expected exit code beside the command.
- **Two masked-exit shapes are still misjudged** (`criterion_hazards/masked_exit.rs:53`): `cargo build && (cargo test ||
  true)` is flagged although a failing build still fails it, and `cmd || true 2>/dev/null` is not flagged. Both are rare;
  they are recorded in [plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md).

## Memory and Hook Follow-Ups After the Doctrine-Truth Plan

Reviewer suggestions the plan left unimplemented:

- `commit-guard.sh` (`remind_memory_usage`) and `learning-validator.sh` (`check_memory_usage`) can disagree for one stage: the first guards the stage id only against empty and `/` and counts a journal of more than 10 lines as entries, the second uses a character whitelist and looks for a `###` entry. Both are advisory and read-only, and the 10-line threshold equals at least one entry only while the journal header (`fs/memory/storage.rs::journal_header`) stays 8 or more lines. Align the guards if either becomes blocking.
- The post-commit box (`remind_memory_capture` in `post-tool-use.sh`) opens with "Record what this work taught you" and only its last line tells a knowledge-distill stage otherwise. A stage-type check would make the box say one thing per stage, if the hook can obtain the stage type.
- `fs/memory/parser.rs::parse_journal` keeps a `## Summary` branch that no code writes any more. The no-backward-compatibility rule argues for deleting it.
- `orchestrator/signals/tests_doctrine_commands.rs` scans the signal templates and skills but not `README.md`, `agents/*.md` or `loom-hooks/_subagent-preamble.txt`, which also teach loom commands.
- `fs/plan_lifecycle.rs::archive_run_state_before_done` repeats `archive_state_dir`'s two-line plan-id lookup because it already holds a `WorkDir` and an explicit main root; take a plan-id helper from `fs/memory/archive.rs` if a third caller appears. `archive_state_dir` has no direct test for its derived plan id and main root: the clean and init tests only check that an archive appears, so an archive named `default-<ts>` would pass them.
- `v2_lints/bookends.rs` imports `StageDefinition` and `StageType` on separate lines from `crate::plan::schema`; one `use crate::plan::schema::{StageDefinition, StageType};` matches the test file.

## Execution Graph And Forward Guard Gaps

> mark_queued status skip; macOS forward-guard branches unrun

## Execution Graph: `mark_queued` Skips Node's Own Status

`ExecutionGraph::mark_queued` (`loom/src/plan/graph/mod.rs:247-285`) checks only the
file-declared stage's dependency list (each dep must be `Completed` and `merged`) before
unconditionally setting `node.status = StageStatus::Queued`; it never checks the node's own
current status first. A stale in-memory graph -- e.g. a daemon still running plan P5 after
`.loom/work/` was replaced by `loom init` for plan P6 -- can force a `Completed` node straight
back to `Queued` for any P6 stage that shares an id with a no-deps P5 stage. Not yet fixed;
candidate remedies: validate `node.status` is a legitimate pre-queued state before overwriting
it, or have callers refuse to sync when the file's declared deps disagree with the graph node's
own dependency list.

## Codex Forward Guard: macOS Stage-Evidence Branches Were Never Executed

`loom_stage_evidence` (`loom-hooks/_codex_forward.sh`) decides whether `codex-forward-guard.sh` enforces anything. Its Linux branches are covered by `loom-hooks/tests/codex-forward-guard-stage-evidence.sh` with real ancestry and a real `bwrap`. Its macOS branches were written and reviewed on Linux and have NOT been run. Background: [The forward guard engages only inside a loom stage](../architecture/codex-plugin.md).

**To do on a Mac, before relying on the guard there:**

- Run `bash loom-hooks/tests/codex-forward-guard-stage-evidence.sh` and `bash loom-hooks/tests/run-all.sh`. The Seatbelt case (guard under `/usr/bin/sandbox-exec -p '(version 1)(allow default)'` with a clean env, expects exit 2 naming `sandbox confinement (seatbelt)`) only runs on Darwin; on Linux it prints `SKIP`.
- E2 parent walk: `_loom_parent_pid` parses `/bin/ps -o ppid= -p <pid>`. Confirm the value is a bare integer after the `read -r` trim under bash 3.2.
- E2 environment read: `_loom_pid_env_has_stage_var` matches `LOOM_STAGE_ID=`, `LOOM_SESSION_ID=`, `LOOM_WORK_DIR=` in `/bin/ps eww -o command= -p <pid>`. Confirm `ps eww` prints the environment of the user's own `claude`/node process on the macOS versions in use (SIP and hardened-runtime processes can hide it), and that the nested-session test (an ancestor with `LOOM_STAGE_ID`, guard run under `env -i`) exits 2.
- `/bin/ps` is setuid root and may be refused inside a Seatbelt profile. If it is, E2 yields nothing there and E3 must carry the case: check that a guard run inside the stage sandbox with a scrubbed env still exits 2 through the `sandbox-exec` refusal probe.
- E3 probe: confirm `/usr/bin/sandbox-exec` and `/usr/bin/true` exist and that the probe SUCCEEDS (no evidence) in an ordinary unsandboxed session, so the stock Codex plugin is usable there: a `codex:codex-rescue` call in a plain session must not be blocked.
- The Rust test `forward_guard_allows_when_no_stage_evidence_exists` (`loom/src/fs/permissions/hooks/policy_tests_stage_gate.rs`) skips only on LOOM_*in its own env or `/proc/1/comm == bwrap`. Under a macOS Seatbelt with no LOOM_* it would FAIL instead of skipping; add a Seatbelt skip if that combination occurs in practice.

**Known limits, all platforms:** a nested `claude` whose launcher controls its environment can set `BASH_ENV` or a config directory with no hooks, which no hook can police (sandbox policy's job); a plain session inside the user's own bubblewrap or Seatbelt wrapper counts as confined and stays blocked.

## check_ignore Git Discovery Is Pinned

- `loom/src/verify/criteria/cache_ignore.rs:87,95` runs `check-ignore` through `checkout_git` and its pinned `git.run`. Fixed in tree.

## DIGEST_ENV Hides The Excludes File

- `loom/src/host_verify/digest.rs:36-40` points `XDG_CONFIG_HOME` at a nonexistent path, so git default excludes file is not read. Fixed in tree.

## hung_latch Module Doc Link

- `loom/src/orchestrator/monitor/hung_latch.rs:21` links `host_job_holds_session`; `host_job_holds_escalation` stays `cfg(test)`. Fixed in tree.

## Network Config Test Name

- `loom/src/models/stage/tests/network_config.rs:4` is `default_network_config_denies_unix_sockets`. Fixed in tree.

## Verification Rules AF_UNIX Example

- `skills/loom-plan-writer/references/verification-rules.md:43-46` frames the AF_UNIX limit as the session sandbox `allow_all_unix_sockets`. Criteria run in the host job bubblewrap, which does not forbid AF_UNIX. Open.

## Pinned Git Gitlink Route Is Unverified

- `loom/src/git/worktree/pinned.rs:107`: a pinned `git status` or `git diff` runs `git status --porcelain=2` inside a gitlink (160000) entry, using the nested repo config, so a `filter.<name>.clean` there could run outside the sandbox. Not reproduced. Probe: add a gitlink to a stage index and check for a marker file.

## Observer Source Uses Path-Only Locate

- `loom/src/verify/review/observer.rs:96` `source()` calls `locate(worktree)` on the given path, not `enclosing_stage`; a subdirectory of a stage worktree would get `WorktreeGit::discovered`. Every current caller passes the worktree root. Open, low.

## Redundant Canonicalize In Observer

- `loom/src/verify/review/observer.rs:146` and `:168` canonicalize again in `enclosing_stage`, and `locate` canonicalizes each ancestor. Correct, redundant syscalls. Open, low.

## Wait Request State After Writeln Failure

- `loom/src/host_verify/wait.rs:206-207`: a failed `requested` write after `write_state` leaves the request state, so a rerun follows a ticket never relayed until `REQUEST_GRACE` ends. Needs closed stdout to trigger. Open, low.

## Duplicate Refusal Import In Freeze

- `loom/src/commands/stage/contracts/freeze.rs:18` imports the refusal as `host_refusal`, so the clash is resolved. Fixed in tree.

## Hook Rejects Commands That Name Stage Completion

- `loom-hooks/loom-control-complete.sh` (PreToolUse) rejects any Bash command whose text contains a stage-complete-like phrase, such as a Python heredoc that mentions "complete" near "request" or "stage". Put the edit script in a scratch file with the Write tool, or use the Edit tool.
- A subagent Bash command that names `src/commands/stage/complete_relay.rs` is refused as an unpinned `stage complete` call. Read such files with the Read tool.
- `subagent-verify-guard.sh` refuses `cargo build --all-targets` in a subagent even when the brief asks for it. Briefs should name only the scoped `cargo test` check as runnable.
