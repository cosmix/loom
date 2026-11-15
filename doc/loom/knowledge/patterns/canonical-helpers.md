# Canonical Helpers

>

## Canonical Helpers

Each cluster below has one implementation under `loom/src/`. A call site routes to it. A site that
keeps its own copy carries a one-line comment naming the canonical helper and the reason it
differs; a copy without that comment is a defect. Helpers are deleted at the old site, never
wrapped or re-exported.

| Cluster | Helper | What it guarantees |
| --- | --- | --- |
| Current branch | `git/branch/operations.rs` `checked_out_branch_with` (`current_branch_with` maps detached to `"HEAD"`) | reads `symbolic-ref -q HEAD`; `Ok(None)` is detached, `Err` is a git failure; no tag ambiguity |
| Stage id from a worktree | `commands/common/mod.rs` `detect_stage_id_in` | resolves `loom/<id>` from the checked-out ref; a tag named `loom/<id>` never changes the answer |
| Repo root and HEAD | `git/revision.rs` `toplevel`, `head_commit`, `require_head_commit`, `has_commits` | `head_commit` is `None` only on an unborn branch; a git failure is an `Err` |
| Change listing | `git/changes.rs` `changed_names`, `changes_since(git, base, filter)`, `untracked_names`, `merge_base`, `read_only` | `-z`, `--no-renames`, `--no-ext-diff`, untrimmed, UTF-8 strict; a working-tree diff runs on a private copy of the index |
| Status parsing | `git/status_entries.rs` `parse_porcelain_v1_z` over `WorktreeGit::run_checked` | strict: NUL-terminated, validated header, rename/copy origin, error on a non-UTF-8 path |
| Blocking a stage | `orchestrator/core/stage_block.rs` `Orchestrator::block_stage` | persists `Blocked`, releases the session, marks the scheduler graph, logs any failure |
| Plan metadata | `plan/parser/validation.rs` `parse_metadata`, `parse_error_text` | schema parse with retired-field guidance; the error text is the root cause without a context line |
| Frontmatter boundary | `parser/frontmatter.rs` `split_frontmatter_raw` | a delimiter is `---` plus optional trailing whitespace; a missing opening delimiter is an error |
| Append | `fs/safe_fs/append.rs` `safe_open_append(dir, relpath)`; `fs/jsonl_spool.rs` `append` | no-follow walk, mode 0600, single-link regular file, exclusive flock; the anchor directory must exist and must not be a symlink |
| JSONL read | `fs/jsonl_spool.rs` `read_complete_lines` | `O_NOFOLLOW`, regular file; a torn final line is skipped; byte and row caps, over the cap is an error |
| Atomic write | `fs/safe_replace.rs` `publish_in_dir[_reporting]`, `open_parent_at(BorrowedFd, rel)`; `fs/cache_io.rs` `atomic_write` | temp file plus rename, file and directory fsync; `cache_io` serves owners whose parent directory may be a symlink |
| Bounded read | `fs/safe_read.rs` `read_bounded`, `read_open_bounded`, `single_link_regular_file`, `is_regular_no_follow` | no symlink follow; a file over the limit is refused, never truncated |
| Lexical path | `fs/lexical.rs` `normalize_lexically` | `None` on any `..` that climbs above a relative start or above `/`: fail closed |
| Text, hash, time | `utils.rs` `sha256_hex`, `parse_rfc3339_utc` (`Option`), `truncate`, `truncate_for_display`, `format_elapsed`; `utils/text.rs` `single_line` | the ellipsis counts toward the budget; `single_line` strips format and bidi characters |
| Process groups | `process/owned.rs` `kill_group`, `signal_group_id`, `wait_with_deadline` | `signal_group_id` refuses group <= 1 and the caller's own group, returns a bool |
| Crash backoff | `orchestrator/retry.rs` `crash_backoff(retry_count)` | one place for `CRASH_BACKOFF_BASE_SECS` and `_MAX_SECS` |

### Kept copies and why

| Copy | Canonical helper it differs from | Reason |
| --- | --- | --- |
| `fs/plan_lifecycle/commit.rs`, `git/stage_commit.rs`, `git/merge/resolved.rs` branch reads | `checked_out_branch_with` | compare the full `refs/heads/<name>` ref; `stage_commit` also separates detached from a git failure |
| `host_verify/digest.rs` `head_in` | `head_commit` | runs under `DIGEST_ENV` through `read_isolated` |
| `verify/contracts/changes.rs` `git` (read-only runner) | `git/changes.rs` `read_only` | `git` must not depend on `verify::contracts`; routing the remaining callers is open work |
| `fs/knowledge/catalog/evidence.rs` bounded probes | `changed_names` | evidence listings carry an output cap the shared helper lacks |
| `models/forward_receipt/locator.rs` `read_bounded_prefix` | `read_bounded` | checks effective-user ownership on the read fd and returns a prefix of a larger file |
| `hooks/events.rs` `read_recent_events`, `telemetry` `read_events` | `read_complete_lines` | the logs have no rotation; a byte cap would turn log size into a read failure. Revisit when either log gets a retention bound |
| `fs/locking.rs` `atomic_write_locked` | `safe_replace::open_parent` | writers run under a worktree's symlinked `.loom/work`; it routes to `cache_io::atomic_write`, which follows a symlinked parent |
| `orchestrator/core/crash_handler.rs` `persist_blocked_crash` | `block_stage` | conditional under the stage lock and bumps `retry_count` |
| `orchestrator/coherence.rs` `block_incoherent_stage`, `stage_service/control_block.rs` `apply_block` | `block_stage` | run without an orchestrator; the coherence caller syncs the graph |
| `host_verify/child_watchdog.rs` `start`, `pressure/codex_run.rs` `cancel_groups`, registry `any_group_left` | `signal_group_id` | signal the watchdog's own group, need ESRCH versus other errors, or send a null-signal liveness probe |
| `orchestrator/core/event_handler/stage_takedown.rs` `confirm_session_gone` | `wait_with_deadline` | polls `backend.is_session_alive` for a session that is not a `Child` |
| `plan_session/guard/paths.rs` `absolute_lexical` | `normalize_lexically` | adapter returning `Result<PathBuf, String>` with the guard's messages |
| `handoff/completion/checkpoint.rs` | `parse_rfc3339_utc` | its errors carry chrono's message and tests match that wording |
| `commands/plan/verify/report.rs` `verify_text` | `parse_metadata` | schema parse without retired-field guidance |

### Properties of the git helpers

- `git diff` against the working tree rewrites `.git/index` whatever `--no-optional-locks` says;
  only `git status` honours it. `changed_names` with no `to` therefore runs git on a private copy
  of the index with the mtime kept for racy-git detection and `core.splitIndex=false`.
- `git/runner.rs` strips `--ignore-submodules=none` from every `git diff`, so `DiffNames.all_submodules`
  has no effect through `WorktreeGit`; commit-to-commit diffs still list gitlinks.
- `--no-renames` lists a pure `git mv` as `A` for the new path and `D` for the old one. Wiring
  detection (`changed_files` with `added_only`) therefore sees a moved file as added and checks its
  wiring; duplicate detection reports nothing because the old path is gone from the graph.
- `IndexBefore::read` fails when HEAD cannot be read; treating a git failure as an unborn branch
  would mark every index entry as user-staged.
