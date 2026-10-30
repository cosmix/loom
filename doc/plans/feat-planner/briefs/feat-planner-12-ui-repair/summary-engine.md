# Stage summary-engine

Read `common.md` first (its rules section applies; there is no visual work here). Working
directory `loom/`; paths are repository-relative.

The user asked for three changes to summary generation:
1. Summaries run on haiku at effort high by default; today overviews and the background summary
   session run on sonnet.
2. A summary session sometimes sits with its opening prompt typed into Claude Code's input box
   but never submitted.
3. The user chooses whether summaries run on Claude Code or on Codex, each with its own model
   and effort.

## E1: configuration (codex gpt-6.1-sol/high through loom-codex-forwarder, in the file-sized units the stage description lists). Runs first; E3 builds on it.

**Files owned:**
- `loom/src/user_config/keys.rs`
- `loom/src/user_config/plan.rs`
- `loom/src/user_config/plan/tests.rs`
- `loom/src/user_config/tests.rs`
- new `loom/src/user_config/plan_keys_tests.rs`
- `loom/src/fs/work_dir/config_sections/plan_config.rs`
- `loom/src/planning/summary/config.rs`
- `loom/src/planning/summary/config/tests.rs`
- `loom/src/planning/guidance/models.rs`
- `loom/src/planning/guidance/models/tests.rs`
- `plan-plugin/agents/summary.md`
- `plan-plugin/agents/overview.md`
- `web/src/api/fixtures/config.json`
- `web/src/components/settings-model.ts`
- `web/src/components/settings-plan-section.test.ts`
- `README.md` (the `[plan]` key table only)
- `doc/product/feat-planner/section_04.md` ("Models")
- `doc/product/feat-planner/section_09.md` (the summary rows and "Models and effort")

Defaults (`loom/src/user_config/plan.rs:9-15`): document summary haiku/high, overview
haiku/high. Change `DEFAULT_PLAN_DOCUMENT_SUMMARY_EFFORT` to `high` and
`DEFAULT_PLAN_OVERVIEW_MODEL` to `haiku`. The background summary session uses the overview
pair (`planning/guidance/models.rs:29-61`), so it moves to haiku/high as well. Update the agent
frontmatter placeholders (`plan-plugin/agents/*.md:5-6`) to match; each must keep exactly one
`model:` and one `effort:` line (`guidance/plugin.rs:94-107` rewrites them).

New keys, each a `KeySpec` row next to the existing four (`keys.rs:218-245`) plus the matching
`PlanSection` field, `parse` line, getter, default constant, `plan_value_of` arm, `plan_toml`
line, and `PlanConfig` field with accessor (`plan_config.rs:12-70`, `deny_unknown_fields`):

| Key | Kind | Default | Help |
| --- | --- | --- | --- |
| `plan.summary_backend` | `Enum(&["claude", "codex"])` | `claude` | Which agent builds summaries and overviews: Claude Code or Codex |
| `plan.summary_codex_model` | `Enum(crate::codex::CODEX_MODELS)` | `gpt-6.1-terra` | Codex model for summaries when the backend is codex |
| `plan.summary_codex_effort` | `Enum(crate::codex::CODEX_EFFORTS)` | `high` | Codex reasoning effort for summaries when the backend is codex |

`SummaryModels` (`planning/summary/config.rs:23-42`) gains `backend`, `codex_model` and
`codex_effort`, resolved project tier → user tier → default like the others.

Traps ([why](../../../../loom/knowledge/mistakes/plan-server-delivery.md#adding-plan-keys-breaks-goldens-and-the-size-gate-outside-the-owned-files-2026-10-28)):
- `user_config/tests.rs` is at about 400 lines. Put the new `value_of` arms' assertions in the
  new `plan_keys_tests.rs` (declared `#[cfg(test)] mod plan_keys_tests;` beside the existing test
  module) instead of extending it. Changing the existing arms at `tests.rs:349-352` for the new
  defaults is an in-place edit.
- `plan/tests.rs:42-48` is the `plan_toml` golden; update it.
- `web/src/components/settings-plan-section.test.ts:9-26` expects exactly four pair rows; it
  becomes the four pairs, plus a `summary_codex` pair, plus a `summary_backend` single row.
  Add a `ROW_CAPTIONS` entry for each new row in `settings-model.ts`.
- `web/src/api/fixtures/config.json` repeats each value four times (default, user, project,
  effective); add the three keys in `KEYS` order and change the two changed defaults.
- Existing tests that pin the old defaults:
  - `user_config/tests.rs:349-352`, `:387`;
  - `user_config/plan/tests.rs:10`, `:23`;
  - `planning/summary/config/tests.rs:8-14`, `:31-32`, `:53-54`, `:76-77`;
  - `planning/guidance/models/tests.rs:19-21`, `:49-51`, `:125-127`.

  Update each to the new default in place.
- `--effort high` with `--model haiku`: confirm the installed `claude --help` lists `--effort`
  without a model restriction, and record the result with `loom memory note`.

Proof: `cargo test --lib user_config::` and `cargo test --lib planning::summary::config`, once each.

## E2: the opening is submitted (opus, loom-senior-software-engineer, effort xhigh)

**Files owned:**
- `loom/src/plan_session/tmux_io.rs`
- `loom/src/plan_session/tmux_io_tests.rs`
- `loom/src/plan_session/bridge/flow.rs`
- `loom/src/plan_session/bridge/fake_host.rs`
- `loom/src/plan_session/bridge/tests_queue.rs`
- new `loom/src/plan_session/bridge/tests_opening.rs`
- `loom/src/plan_session/tmux_host.rs`

How a summary (and authoring) session gets its opening today:
- An interactive `claude` runs in a private tmux server.
- After the first `SessionStart{startup}` event marks the state Idle (`plan_session/state.rs:200-205`),
  `take_paste` (`bridge/flow.rs:50-96`) hands the opening to `TmuxHost::deliver`
  (`tmux_host.rs:16`).
- That calls `paste_text` (`tmux_io.rs:26-37`), which runs three tmux processes back to back:
  `load-buffer`, `paste-buffer -p -d`, `send-keys Enter`.
- Nothing waits between the paste and Enter, nothing checks that Enter took, and nothing
  retries.
- A `UserPromptSubmit` hook within `delivery_timeout` (10 s, `bridge/mod.rs:105`) confirms it.
  Otherwise `time_out_delivery` (`flow.rs:196-216`) marks the item undelivered and sets `paused`.
- The opening was already `take()`n (`flow.rs:104`), so a summary session then idles until the
  scheduler's 10-minute `NO_SEAL_TIMEOUT`.
- The e2e run `a7d644c2` stalled the same way (`architecture/planning-session-bridge.md`).
- Pressure sessions put the opening on argv and are unaffected (`plan_server/pressure/driver/step.rs:205`).

Required behaviour:
1. `paste_text` waits 150 ms after `paste-buffer` before `send-keys Enter`. The delay is a named
   constant with a comment saying why.
2. For an **opening** delivery only (never a user's chat message, which may be half-typed in the
   box), when no `UserPromptSubmit` has arrived 2 s after Enter, press Enter again: only Enter,
   never a second paste, because the text is already in the box. Do this up to two more times, 2 s
   apart, all inside `delivery_timeout`. Add a `press_enter` (or reuse `press`) to the host trait
   that `fake_host.rs` records.
3. When the opening still is not submitted at `delivery_timeout`, a **Summary** session fails
   with the reason "opening not submitted", through the same failure path a failed paste uses
   (`pasted()` at `flow.rs:172-196`). The scheduler then requeues its ids (`session_failed`). An
   authoring session keeps today's undelivered and paused behaviour, with the opening
   resendable.
4. Ticks stay at 200 ms; no blocking sleeps on the driver thread beyond the 150 ms in `paste_text`
   (which runs where the paste already runs). If `paste_text` runs on the driver thread, move the
   delay and the Enter to a short-lived helper thread and record the decision.

Reproduce first: a test with the fake host that withholds `UserPromptSubmit` must fail on
today's code before the fix. Tmux tests skip inside the stage sandbox; the fake host is the
proof.

Proof: `cargo test --lib plan_session::`, once.

## E3: Codex summary backend (codex gpt-6.1-sol/high through loom-codex-forwarder, in units). Starts after E1 and the orchestrator's sandbox probe.

**Files owned:**
- new `loom/src/plan_server/summaries/codex_spawner.rs`
- new `loom/src/plan_server/summaries/codex_spawner_tests.rs`
- `loom/src/plan_server/summaries/mod.rs` (the `install` selection only)
- `loom/src/plan_server/summaries/spawner.rs` (only to share helpers)
- new `loom/src/planning/guidance/text/summary_codex.md`
- `loom/src/pressure/codex_run.rs` (only to make `codex_args` and `spawn_codex_background`
  reusable without changing pressure's behaviour)

Pressure runs Codex as `codex exec --sandbox workspace-write -m <model> -c
model_reasoning_effort=<effort> -C <repo_root> <prompt>` (`pressure/codex_run.rs:195-228`). It
has stdin null, a log file, fallback within a model family
(`spawn_codex_with_fallback`, `:111-135`) and the user's own Codex login.

A summary job must:
- write `drafts/<id>.md` under the runtime directory (`RuntimePaths.drafts`,
  `planning/summary/claims.rs:111-126`; outside the project, under the loom user dir);
- run `loom plan summary brief <id>` and `loom plan summary seal <id>` with `LOOM_PLAN_STATE` and
  `LOOM_PLAN_SESSION` set, so it holds its claims (`seal.rs:113-127`).

Seal validates the links and computes every hash itself, so the model cannot corrupt them.

Build:
1. `CodexSummarySpawner` implements the scheduler's `SessionSpawner`
   (`plan_server/summaries/scheduler.rs:24-29`):
   - `start_summary(ids)` claims as the Claude spawner does;
   - it spawns one `codex exec` with the summary prompt for those ids, the resolved
     `summary_codex_model` and `summary_codex_effort`, `-C <root>`, and write access to the
     runtime directory;
   - it publishes `Started`, and on exit `Ended` (success or failure with the classified
     reason) to the scheduler listener;
   - `stop` kills the process group.

   Pin the command builder as
   `pub fn codex_summary_args(model: &str, effort: &str, root: &Path, runtime: &Path, prompt: &str) -> Vec<String>`.
2. **Sandbox.** The orchestrator, before dispatching E3, runs `codex exec --help` on the host and confirms how to grant a directory outside
   `-C` (an `--add-dir` style flag, or `-c sandbox_workspace_write.writable_roots=[...]`). Use
   what the installed CLI documents; the orchestrator records it with `loom memory decision` and
   passes it to every E3 unit. If no documented way exists, the orchestrator blocks the stage with
   `loom stage block` and the evidence. Never widen
   to `danger-full-access`.
3. **Prompt.** `summary_codex.md`, embedded with `include_str!`, gives the full instructions
   inline (no Codex skill dependency):
   - the ids;
   - the brief-draft-seal sequence per id, documents before overviews (read
     `plan-plugin/skills/summaries/SKILL.md`, `plan-plugin/agents/summary.md`, `overview.md`
     and `planning/guidance/text/summary_brief.md` and carry their rules over);
   - "never write under `doc/`".
4. **Config guard.** Snapshot `.claude/` and `.codex/` configuration before and after, as
   pressure does (`plan_server/pressure/config_guard.rs`), and fail the job on a change.
5. **Selection.** `summaries::install` picks the Codex spawner when
   `SummaryModels.backend == "codex"`, re-read per job like `RealSpawner` (`spawner.rs:44-76`
   reloads `GuidanceModels` each time). Keep the `install_all` ordering constraint.
6. **Cost.** A Codex job has no Claude transcript; record no cost for it (the ledger permits
   absent components).
7. The tmux attach link does not apply to a Codex job. The session record shows the log path
   instead.

Proof: `cargo test --lib plan_server::summaries::`, once.
