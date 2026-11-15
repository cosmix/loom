# Consolidation Followups

>

## Open Items After the Helper Consolidation

Review suggestions and kept copies the dry-consolidation plan left open. Helpers and the reason
each copy stays are in [canonical-helpers](../patterns/canonical-helpers.md). Paths are under
`loom/src/`.

## Symlink, FIFO and size-cap gaps

- `host_verify/store_fs.rs` `append_locked` and `commands/subagents/wait/lease_fs.rs`
  `validate_target` open without `O_NONBLOCK`, so a FIFO planted at the job file or `lease.json`
  blocks the wait; `append_locked` also skips `safe_read::single_link_regular_file` after the flock.
- `fs/memory/storage.rs` `create_journal` and `orchestrator/adjudication/feedback.rs`
  `write_notice` follow a symlink and truncate or overwrite its target; `safe_fs::safe_locked_write`
  already does a no-follow, locked, single-link write.
- `hooks/events.rs` and `telemetry/mod.rs` keep unbounded readers that follow a symlink at the log
  name; `fs/jsonl_spool.rs` `read_complete_lines` accepts a multi-link file although the appender
  refuses one.
- `subagent_lifecycle/store.rs` and `codex_lifecycle/ledger.rs` call `symlink_metadata` then open; a
  file removed between the two reads as empty rather than `NotFound`.
- `verify/criteria/confine_protected.rs` no longer lists a symlinked secret in `hidden_files`;
  confirm the old predicate also skipped one.
- Several writers still use `NamedTempFile::persist` without the fsync `publish_in_dir` does:
  `host_verify/wait_state.rs`, `verify/contracts/refusal.rs`,
  `commands/stage/contracts/freeze_pending.rs`, `orchestrator/adjudication/session.rs`,
  `fs/permissions/trust.rs`, `fs/permissions/codex_hooks.rs`, `git/hooks.rs`.
- `take(MAX)` without a `+1` check silently accepts a truncated file in
  `status/data/execution_models.rs`, `verify/review/gate.rs`, `plan_server/run/abandon/marker.rs`,
  `verify/contracts/refusal.rs` and `fs/daemon_state.rs`; `safe_read::read_bounded` refuses an
  over-limit file.
- `fs/safe_read.rs` `OverLimit` messages say "verification limit" although `read_open_bounded` is
  general purpose; only `relay/output.rs` replaces the message.

## Git helpers

- `git/changes.rs`: no test rejects removing `set_modified`, so a racily clean file (same second,
  same size) could be trusted on stat data and missed; add a test that sets a file mtime equal to the
  index mtime. A third 40/64-hex object-id check sits beside `git/merge/control_paths.rs` and
  `context/worktree_graph.rs`; share one. `git/merge/checkout_stash.rs` keeps a second
  private-index-copy mechanism beside `PrivateIndex`.
- `git/merge/control_paths.rs` `hooks_dir_prefix`: a relative `hooksPath` outside the repository
  errors and holds every merge, while an absolute outside path returns `None`; reconcile.
- `fs/knowledge/catalog/evidence.rs` `parse_status_paths` still hand-parses porcelain v1 `-z`
  lossily; `verify/contracts/changes.rs` keeps its own read-only runner for `diff_from_base`,
  `ignored_paths`, `harness_files` and `status`, used by `integrity/count.rs`, `integrity/edits.rs`,
  `impact_tests/base.rs` and `review/base_move.rs`.
- `git/branch/operations.rs`: a HEAD symref outside `refs/heads` (for example `refs/tags/x`) is
  returned as a branch name; `Ok(None)` or `Err` is stricter. `commands/stage/acceptance_runner.rs:56`
  and `handoff/completion/content.rs:108` run the same branch-ownership check with different
  messages.
- `git/stage_commit.rs:231` and `fs/plan_lifecycle/commit.rs:131` justify the kept copy by saying
  the canonical helper cannot tell detached from a git failure; it can. The reason is the full-ref
  comparison and the commit refusal types.
- `git/merge/stage_tests.rs:173` and `operations_start_point_tests.rs:55,115` assert through
  `rev-parse --abbrev-ref HEAD`; compare `symbolic-ref -q HEAD` with `refs/heads/<name>`.
- `commands/plan/check.rs:131` drops the `toplevel` error, so a git timeout prints "not inside a git
  repository"; include `{error:#}` as `plan_server/root.rs` does.
- `commands/init/execute.rs:125` depends on `commands::run::checks::print_repo_bootstrap`; a
  neutral home would be `commands::common` or the git module.

## Process groups and blocking

- `plan_server/summaries/codex_spawner/registry.rs`: `signal_group_id` returns a bool, so a refused
  forced SIGKILL leaves a codex group running with no log and `ask_to_stop` no longer prints the
  errno; `discard` has no `child.kill()` fallback before its blocking wait. `pressure/codex_run.rs`
  logs non-ESRCH failures and could serve as the model.
- `verify/criteria/spawn_observer.rs:51`: a `try_wait` I/O error goes straight to the final kill and
  logs "did not stop".
- `orchestrator/core/stage_launch_policy.rs:56`: if `start_knowledge_stage` already blocked the stage
  and returned `Err`, the second `block_stage` is refused (Blocked to Blocked) and logs a spurious
  "failed to persist Blocked state" error. `orchestrator/core/session_lifecycle.rs:138,180` print an
  `eprintln` that drops the cause and repeats `block_stage`'s `tracing::error`.
- `orchestrator/core/provision_gate.rs`: a before-listing that fails to parse is skipped with no log,
  so a non-UTF-8 file planted by an earlier attempt silently turns the unignored-file check off; the
  `NON_UTF8_REASON` block applies to any after-listing parse failure, a malformed header included.
- `orchestrator/core/event_handler/stage_takedown.rs:256` keeps `confirm_session_gone` without a
  comment naming `process::wait_with_deadline`.
- `orchestrator/core/recovery.rs:130` repeats "base 30s, max 300s" from `CRASH_BACKOFF_*_SECS`;
  `daemon/server/launch.rs:197` repeats `DAEMON_CHILD_CHECK`.

## Parsing, paths and text

- `plan_server/pressure/config_guard.rs:347` normalizes by component and keeps its own copy of the
  `..` policy; the other fail-closed copies route to `normalize_lexically`.
- `plan_session/guard/paths.rs` `rule_path` drops a read-deny entry silently when `absolute_lexical`
  fails; `ReadRules::new` returns a `Result`, so propagate it. Entries are loom-generated, so this
  is not exploitable today. `plan_session/guard/mod.rs:305` `claude_tmp_dir` returns `None` when
  `TMPDIR` cannot be normalized; confirm the caller reads `None` as no allowance.
- `plan_session/bridge/team.rs:44` falls back to `project_root`, which is safe only because a root
  containing `..` never matches a normalized path; state that in a comment.
- `planning/guidance/plugin.rs:122` and `planning/summary/format.rs:241` match the error string "No
  frontmatter delimiter found at start of content" from `parser/frontmatter.rs:83`; use a shared
  constant or a typed error. `planning/summary/format.rs:285` `parse_draft` still probes for `---`
  itself. `completions/dynamic/stages.rs:80` `status_matches` closes frontmatter on any indented
  `---`. `parser/frontmatter.rs:82` `first_line.trim()` accepts leading and Unicode whitespace on the
  opening delimiter, looser than the stated trailing-whitespace rule. `parser/markdown.rs:146` has no
  case for an unindented `--- # c` line.
- `planning/summary/brief.rs:170`: frontmatter-only text without a trailing newline prints the
  closing `---`.
- `utils.rs:270` `truncate` returns three characters for `max_chars < 3`, over the budget the
  ellipsis is meant to count toward. `git/merge/fast_forward.rs:203` cuts at `DETAIL_LIMIT` with no
  ellipsis; `truncate_for_display` is the canonical form. `commands/stage/contracts/freeze_follow.rs:68`
  flattens the marker note with `split_whitespace`; use `utils::text::single_line` if it can carry
  agent text.
- `commands/plan/verify/report.rs:179` defines a second `parse_metadata` (an `Option`, no
  retired-field guidance) under the name of `plan::parser::parse_metadata`; rename it.
  `commands/stage/admin_proof.rs:204` has a `resolve_work_dir` that canonicalizes, a different
  behaviour under a name the one-definition sweep matches.
- `handoff/completion/content_inputs.rs:63`: a declaration climbing out of the repository no longer
  marks the in-repo file of that name as a declared input, so it can fall under a completion
  exemption where the old copy checked it. Plan decision A9 chose this.
- `codex_lifecycle/authorization.rs:59` labels shared `job_record` validator errors "companion
  <field>" where the file says "Codex authorization <field>". `models/forward_receipt/job_record.rs:168`
  now applies the `..` check to companion job records (`workspaceRoot`, `request.cwd`), a deliberate
  tightening.
- `verify/review/prose_tests.rs:111` compares `behaviour_digest` with the `sha256_hex` it calls; a
  pinned literal digest would catch an encoding regression.

## Waiting on codex forwarders

`loom subagents watch` exits 5 ("expected exactly one Codex execution, found 0", or "worker set does
not resolve to one Claude parent UUID") when armed before the codex forwarders register their
jobs, and the mistakes files already record it eight or more times
([subagent-liveness-and-watch](../mistakes/subagent-liveness-and-watch.md)). Proposal: the command
retries the worker resolution for a short window after a forwarder spawn instead of exiting 5, so no
agent has to learn the ordering.

## Git Fixture Guard Gaps

- `loom/tests/test_git_guard_contracts.rs`: a `bespoke-git-fixture` marker exempts the whole file from all three rules (identity, runner, isolation), so later plain setup in a marked file passes unchecked. Scoping the exemption per rule or to the lines near the marker would close it.
- Rule 3 (inline test module) misses `#[cfg(test)] mod tests {` on one line and an attribute such as `#[allow]` between `#[cfg(test)]` and `mod`; no current file hides this way.
- The guard matches `fn <name>` definitions only: a `use ... as git_ok` alias (`src/git/merge/test_support.rs`, `git/cleanup/tests.rs:14`, `context/refresh/tests_snapshot.rs:5`, `tests_freshness.rs:7`, `tests_source_graph/mod.rs:10`, `context/tests/overlay_key.rs:27`, `context/worktree_graph_tests.rs:11`) keeps a deleted runner name. Callers can import `git` directly once assertion text may change; merge tests mix `git_ok`/`git_out` aliases with direct `test_git::git` imports.
- `failed_git_mutation_reports_command_and_output` (`tests/test_support/git_repo.rs`) runs in the lib, `main.rs` and each integration crate that includes the module; cheap but redundant.
- `isolate_git` in `tests/host_job_fixture/mod.rs:33` is a one-line pass-through to `git_repo::isolate`; callers could use `isolate` directly. `tests/target_ref_hook_contracts.rs:31` `git_as` re-implements `git_with_env` for the `LOOM_SESSION_ID` case. `git_with_env` builds through `git_command` plus `run_command`, not `git_with_program`, so the env is applied after the isolation.
- `.branch("main")` repeats the builder default in `commands/init/base_branch.rs:30,46`. `verify/criteria/tests/runner_tests.rs:24` and `cache_input_runner_tests.rs:22` use `no_seed()`; confirm against `git show 3b9756d02` that the old fixtures had no seed.
- Unisolated `Command::new("git")` remains outside the guard's name list: `orchestrator/core/recovery_tests.rs:149`, `git/branch/status_tests.rs:28`, `git/branch/ancestry.rs:189`, and bare `git init` helpers in `tests/subagent_owned_wait/support.rs:293`, `planning_check_cli_contracts.rs:31`, `planning_summary_support/mod.rs:49`, `planning_guidance_contracts.rs:311`, `integration/plan_verify.rs:606`, `plan_session_bridge_e2e/probes/setup.rs:60`. Host `init.templateDir` and `init.defaultBranch` reach them; the candidate is `TestRepo::builder().no_seed().init_in`.
- `src/git/worktree/pinned_settings_tests.rs:34` and `src/git/user_git_tests.rs:18` run some setup through production wrappers; `crate::test_git::git_unchecked` and `git` fit.
- `host_verify/tests_digest_pinned.rs:157`: six setup assertions were renamed from `run_git` to `git_succeeds`.
- `orchestrator/core/merge_lifecycle_e2e_tests_support.rs:180` `checkout()` stores a trimmed porcelain status; equality comparisons are unaffected, but compare with the old runner if the untrimmed text is ever read.

## Recurring Process Failures Awaiting a Mechanical Check

- `loom check` with `run_in_background` or redirected output recurs across plans ([plan-server-delivery](../mistakes/plan-server-delivery.md), [test-git-support-delivery](../mistakes/consolidation-drift.md)). A PreToolUse hook that refuses `run_in_background` on a `loom check` or `loom stage complete` command would end it.
- A `loom-code-reviewer` round recorded malformed when the report held no `loom-review` block recurs. The brief must say the `SubagentHandback` report itself ends with the fenced block; a `loom plan verify` or hook check on the review brief template is the mechanical fix.
- `adjudication_e2e` (amendment, `dispute_amendment_is_idempotent_under_repeat_apply`, `amendment_cap_exceeded_*`, `kinds::freeze_refusal_accept_*`) failed six tests in the sweep stage sandbox at the base tree as well; the integration-verify full-suite host job passed. The cause is probably the writable-`/tmp` lock path in [verification-v2-followups](verification-v2-followups.md), not the fixture migration; unverified.
