# Pressure

>

## One Pipeline, Two Drivers

`loom::pressure` (`loom/src/pressure/`) is the one pipeline behind `loom pressure` and the planning server's pressure run. `run_pipeline(driver, &PipelineInput)` (`mod.rs`) walks `plan_steps` (`steps.rs`: `Step::{DeleteReport, Pressure{claude, codex}, Address}`, the Claude `/pressure` + Codex `$pressure` pair per round, then `/address`) and calls a `PressureDriver` for each: `delete_report`, `run_pressure`, `run_address`. A `StepVerdict::Stop` deletes the report, writes no record and returns `PipelineOutcome::Stopped`. A step `Err` deletes the report and returns the error. `rounds >= 1` is asserted in `run_pipeline` itself.

| Driver | Where | Runs steps as |
| --- | --- | --- |
| `TerminalDriver` | `commands/pressure/terminal.rs` (+ `spawn.rs`, `paths.rs`, `mod.rs::execute`) | foreground `claude` on the TTY with a marker file watched for completion; Codex in the background with a spinner |
| app driver | `plan_server/pressure/` (`driver.rs`, `driver/step.rs`, `codex.rs`) | a pressure session through the planning bridge per Claude step; `codex exec` beside the `/pressure` session |

Everything shared lives in `loom/src/pressure/`: `models.rs` (`PressureModels::resolve`, free `resolve_for`, `project_pressure_config`, `resolve_rounds`), `paths.rs` (`resolve_plan_path`, `codex_report_path`), `codex_run.rs` (`CodexRun`, `spawn_codex_with_fallback`, `unavailable_line`, `codex_args`), `instruction.rs` (`completion_instruction`, `MarkerHome`), `record.rs`. `commands/pressure/` keeps only the terminal-specific half; `models.rs` and `fallback.rs` are gone. The two drivers resolve rounds differently: the terminal uses `resolve_rounds` (flag, then `[pressure] rounds`, default 1) while `plan_server/pressure/options.rs` defaults the dialog to 2 and validates 1..=10 (open, see [handoff-followups](../concerns/handoff-followups.md)).

## The Record

The pressure pipeline renders a `RecordLine` with date, model names, rounds and missing-review evidence, then calls `record::append_record` before deleting the final report. An empty Codex model renders as `unknown`.

`append_to_text` preserves CRLF, places a record inside an existing unfenced `## Pressure tests` section before the next heading, or appends a new section. It uses the parsed metadata boundary, with a nonfenced marker fallback. The public text renderer preserves those placement rules.

File publication takes the stable per-plan lock also used by amendment and plan-server save. It reads a retained no-follow regular file with an 8 MiB cap and stages a synced sibling candidate. To avoid overwriting a noncooperating edit, it writes once with `O_APPEND` only if the renderer result extends the snapshot. An interior insertion, unsupported file mode or other unsafe case is refused before publication. The path and contents are checked again after writing; a conflict can leave a suffix or partial fragment and is reported for inspection, without destructive rollback.

## The Codex Half

`codex_model_candidates` orders the fallback chain (see [model-and-effort-config](../conventions/model-and-effort-config.md)); `unavailable_line` in `codex_run.rs` is the only judge of "model unavailable". The app's `CodexHalf` (`plan_server/pressure/codex.rs`) snapshots session configuration before Codex starts (`config_guard::snapshot`), records the Codex `ProcessIdentity` (pid plus start time) in the session record again after each fallback attempt, and compares snapshots after the step. A change ends the run with a reason starting `Codex changed session configuration` and writes a `StartBlocks` entry; a Codex still alive after `codex_exit_grace` (10 s) writes the block `Codex did not exit; session configuration cannot be checked`.

`config_guard` fingerprints regular files by SHA-256 through no-follow reads; symlinks by `readlinkat` target plus what an in-repo target serves (file digest, special kind, or the directory's tree), dangling/loop/outside links by target only; FIFOs, sockets and devices by kind, never followed. One Walker-owned work stack with `MAX_ENTRIES = 100_000` and `MAX_PATH_BYTES = 4096` fails closed past either; a real-directory `.claude/worktrees/` (Claude Code's own checkouts) is skipped. A link resolving to the repo root, `.git` or `.loom` is refused.

## Stop, Grace And Recovery

Stop and the holder-away grace go through the shared `driver/live.rs` state (each lock held for one short step). Stop terminates the current Claude session and Codex (SIGTERM, wait at most the exit grace, then the block above) and writes no record. Start-up recovery (`recover.rs`) tears a live pressure session down, terminates a recorded Codex only when its start time verifies against the stored identity (missing evidence never destroys), deletes that run's Codex report and names the run on the terminal. `PressureRunning` (`options.rs`) is the availability flag: set for the whole accepted run, cleared on every end; the ended `pressure_run` frame is published before it clears, so the dialog also refetches on `PlanMessage::PressureOptionsChanged` (sent after the clear and after a successful confirm-config).

Routes (`pressure/routes.rs`): `GET /api/pressure/options` Viewer; `POST /start` and `POST /confirm-config` `Role::HolderOrTake` (after a restart nobody holds authoring, and a persisted Codex block must stay confirmable); `POST /stop` Holder.

## Teammates

Claude Code 2.1.312 writes an in-process teammate's transcript where it writes a subagent's (`<transcript dir>/<session>/subagents/agent-<id>.jsonl`, `agent-<id>.meta.json` with `taskKind: in_process_teammate`). `teammates.rs` registers each with the bridge's `add_token_transcript`; usage is keyed by message id, so a file the bridge also tails is not counted twice. The settings key `teammateMode` is real in 2.1.312; whether a `--settings` file beats the user's own value is unobserved, and the driver's tmux pane check is the runtime guard. Revisit if Claude Code moves the files or drops `taskKind`.

## Terminal Driver Behaviour

`TerminalDriver` keeps `loom pressure`'s output as it was before the pipeline moved: a `Stopped` outcome is `Ok(())` with no completion lines, and stop reasons come from the terminal exit helpers (`spawn.rs::should_stop`, `claude_should_stop`). Codex fallback notes print after the foreground Claude session exits. `execute` resolves rounds with `resolve_rounds` and `render_dry_run` prints the planned steps from `plan_steps`. A Codex attempt leads its own process group, so the terminal's SIGINT never reaches it. `retire_on_signal` holds a `CodexRetirement` (`pressure/codex_run/retirement.rs`) that signals every run started through it on a termination signal; the process exits 130 and refuses any further fallback or run. A SIGKILL of `loom` runs no handler and still leaves Codex running. A finished worker leaves no group to signal, and a group is dropped once `killpg(group, 0)` finds it empty, which narrows and does not close the window for a recycled group id; a straggler that outlives its worker is not killed by a later cancel.
