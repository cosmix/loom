# W3: portable hooks, BSD shims, CI (issue #24, shell side)

Read `doc/plans/briefs/open-issues-19-24/common.md` first. You are a sonnet worker of stage
`platform-portability`. Plan Decisions 6 and 10 are binding.

## Role and issue

On macOS, `wc -c <file` prints a left-padded count (`123`). Five hook sites tested the raw
value against `^[0-9]+$`, so `subagent-stop.sh` exited before it appended a lifecycle row or ran the
review harvest, and `codex-forward-guard.sh` blocked every codex forward ("forwarder identity does
not match exactly one SubagentStart row"). PR #25 (your base) already strips the padding at all five
sites with an inline `| tr -d "[:space:]"`; keep those edits. Your work: the `sha256` helper, the
BSD epoch fallback, the skip ledger that replaces the PR's `hook-skips.log`, the BSD shims, and the
hook suite in CI in both modes.

## Files owned (the plan's W3 row; hook paths are repository-relative, Rust paths package-relative to `loom/`)

`loom-hooks/_lifecycle.sh`, `loom-hooks/codex-forward-result.sh`, `loom-hooks/codex-forward-guard.sh`,
`loom-hooks/subagent-stop.sh`, `loom-hooks/teammate-idle.sh`, `loom-hooks/tests/bsd-shims/wc`,
`loom-hooks/tests/bsd-shims/stat`, `loom-hooks/tests/bsd-shims/date` (new),
`loom-hooks/tests/_bsd_path.sh` (new), `loom-hooks/tests/run-all.sh`,
`loom-hooks/tests/subagent-stop-review-harvest.sh`, `loom-hooks/tests/subagent-stop-heartbeat-lock.sh`,
`tests/worker_evidence.rs`, `tests/worker_evidence/support.rs`, `tests/worker_evidence/setup.rs`,
`tests/codex_evidence/fixture_runtime.rs`, `tests/codex_evidence/happy_path.rs`,
`.github/workflows/ci.yml`. Write nothing else.

## Pinned interfaces (common.md and the plan, quoted)

- "Portable shell helpers live in `loom-hooks/_lifecycle.sh`. `loom_lifecycle_sha256` uses
  `sha256sum`, else `shasum -a 256`. Every bare `sha256sum` goes through it. The BSD epoch fallback
  accepts fractional seconds." Padded `wc` counts are stripped inline (`| tr -d "[:space:]"`, the
  form PR #25 used); there is no `loom_lifecycle_file_bytes` helper and you add none.
- "A skipped `loom-code-reviewer` stop writes one row to
  `.loom/work/subagents/<stage>/stop-skips.jsonl`." Row schema, pinned for W4 who reads it:
  `{"ts":"<UTC ISO>","agent_id":"<id>","agent_type":"loom-code-reviewer","reason":"<code>"}`. This
  replaces PR #25's free-text `hook-skips.log`: no `hook-skips` text remains under `loom-hooks/`
  (the plan has an acceptance grep for it).
- Contract (frozen, a TempDir `bin/` with a padded `wc` first on PATH):
  `loom_lifecycle_resolve_start <work> <stage> <parent> <loom_session> <agent> <expected_type>
  <observed> <hook>` must still resolve one exact start row in `<work>/subagents/<stage>/starts.jsonl`.
  Contract `padded-wc-resolves-the-start-row` already holds at your base because of PR #25's inline
  `tr`. The contract file still freezes red (it names symbols this stage creates, so it cannot
  compile at freeze). Its mutation check is removing `| tr -d "[:space:]"` from
  `loom_lifecycle_resolve_start`; never remove it.
- The hook files are embedded in the binary by `include_str!` constants
  (`loom/src/fs/permissions/constants.rs`); a new top-level hook file would need registering there,
  which is not yours. All new shell goes inside `_lifecycle.sh` (369 lines now; stay at or under
  400) or `subagent-stop.sh`.

## Sites at the base (PR #25 head, `11859505`)

Already fixed by PR #25, do not touch: the padded-`wc` reads in `_lifecycle.sh`
(`loom_lifecycle_resolve_start` `:163`, `loom_lifecycle_transcript_evidence` `:205`,
`loom_lifecycle_journal_ready` `:238`) and in `codex-forward-result.sh` (`load_authorization` `:51`,
`valid_persisted_output` `:86`), each now `bytes=$(wc -c <"$f" 2>/dev/null | tr -d "[:space:]")`.

Yours: `_lifecycle.sh` `:224` bare `sha256sum`, `:105-107` (`loom_lifecycle_epoch`: BSD fallback
parses only the literal `.000Z`). `codex-forward-result.sh` `:188-198` (`event_id`, duplicated
sha256 branches). `subagent-stop.sh`: `:55` dependency check, `:159` digest, and the PR's
`record_reviewer_skip` (`:127-134`, called at `:142` and `:151`), which Task 4 replaces.
`teammate-idle.sh`: `:16` dependency check, `:76` digest.
Not bugs, leave alone: `_lifecycle.sh:211-213`, every `tail -c 1 | wc -l` check, `post-tool-use.sh`,
`knowledge-orient.sh`, `_read_ledger.sh`, `_read_discipline.sh`, `commit-guard.sh`.
`codex-forward-guard.sh:246,327` fails only through `loom_lifecycle_resolve_start`, and
`forward_observed_at` through `loom_lifecycle_epoch`; the PR fixed the first and Task 2 fixes the
second, so `codex-forward-guard.sh` needs no edit unless you find one.

## Tasks

1. **Helpers in `_lifecycle.sh`** (bash 3.2 compatible: no `${x,,}`, `declare -A`, `mapfile`; keep
   regexes in variables): `loom_lifecycle_sha256` reads stdin and runs `sha256sum` when present,
   else `shasum -a 256` (non-zero when neither exists); `loom_lifecycle_have_sha256` is the
   one-line availability test. Replace `:224` with the helper. The `wc -c` blocks keep PR #25's
   inline `| tr -d "[:space:]"`; do not replace them. Run `rg -n 'wc -c' loom-hooks` once at the
   end: a `wc -c` whose value feeds a numeric test and that has no `tr` gets the same inline
   `| tr -d "[:space:]"` (the brief names none beyond the five the PR fixed). Budget: the file ends
   at or under 400 lines.
2. **Epoch fractions**: `loom_lifecycle_epoch` keeps the GNU `date -u -d` attempt; its BSD fallback
   splits the value with a regex into base `YYYY-MM-DDTHH:MM:SS`, optional `.digits` (dropped) and
   zone. `Z`: `date -j -u -f '%Y-%m-%dT%H:%M:%SZ' "${base}Z" +%s`. `+hh:mm`: remove the colon and use
   `date -j -f '%Y-%m-%dT%H:%M:%S%z' "${base}${zone}" +%s`. The equal-second fraction check in
   `loom_lifecycle_resolve_start` (`:188-190`) stays as is. The `+hh:mm` branch runs `date -j -f`
   without `-u`, so the `date` shim (Task 5) must accept `-j -f` with or without `-u`.
3. **Other sites**: `codex-forward-result.sh` `event_id` pipes into `loom_lifecycle_sha256` once
   (delete the duplicate branch); `load_authorization` and `valid_persisted_output` are the PR's
   and stay. `subagent-stop.sh` and `teammate-idle.sh`: dependency check (`:55` / `:16`) uses
   `loom_lifecycle_have_sha256`, digest (`:159` / `:76`) uses `loom_lifecycle_sha256`. The acceptance
   command `rg -q -F 'sha256sum' loom-hooks/subagent-stop.sh loom-hooks/teammate-idle.sh` must find
   nothing, so reword the debug strings and comments there (`subagent-stop.sh:56,165`,
   `teammate-idle.sh:82`: "sha256 digest invalid", not the tool name).
4. **Skip ledger** in `subagent-stop.sh`: a function `loom_subagent_stop_skip <reason-code>` that
   replaces PR #25's `record_reviewer_skip` (delete that function and its free-text
   `hook-skips.log` line; keep its two call sites, now at the `no_unambiguous_start_row` and
   `transcript_unusable` rows below, and its guards: the directory exists and is no symlink, and the
   log file is no symlink). The new function returns unless `$AGENT_TYPE == $REVIEWER_AGENT_TYPE`,
   requires `loom_lifecycle_plain_path "$WORK_DIR/subagents/$LOOM_STAGE_ID" dir` (never creates a
   directory) and a non-symlink `stop-skips.jsonl`, builds the row with `jq -nc` (`ts` from a fresh
   `date -u +%Y-%m-%dT%H:%M:%S.000Z`; it does not reuse `OBSERVED_AT`), and appends it with one
   `{ printf '%s\n' "$row" >>"$file"; } 2>/dev/null || true`. The PR defined its function after the
   `OBSERVED_AT` check; define yours before the first call site (the `transcript_not_plain` skip), so
   every site can reach it. Diagnostic only: it never changes an exit code or output. Call it, after
   the existing `loom_debug`, at every `exit 0` skip that follows the work-dir and stage-binding
   checks. Anchor each site by the `loom_debug` message it sits after, never by a line number:

   | Reason code | Site: the `loom_debug` message (after the `"$HOOK_NAME: "` prefix) |
   | --- | --- |
   | `transcript_not_plain` | `skipping - parent or worker transcript is not a plain normalized file` |
   | `transcript_layout_mismatch` | `skipping - transcript layout, parent UUID, or agent id differs` |
   | `timestamp_unavailable` | `skipping - UTC timestamp unavailable` |
   | `no_unambiguous_start_row` | `skipping - no unambiguous exact SubagentStart row` (start status 1) |
   | `lifecycle_defect` | the `exit 0` of the same `START_STATUS` block when the status is not 1 (no `loom_debug` of its own) |
   | `transcript_unusable` | `skipping - worker transcript is empty, torn, malformed, or changing` (transcript status 1) |
   | `event_digest_failed` | both digest skips: `skipping - event id digest failed` and the invalid-digest skip right after it (reworded by Task 3: its text no longer names the tool) |

   Sites before the stage-binding check (unsafe identity, unavailable or unsafe work directory,
   unbound stage) write no row: there is no trustworthy stage directory yet.
5. **BSD shims** in `loom-hooks/tests/bsd-shims/` (`#!/usr/bin/env bash`, bash 3.2, each finds the
   real tool by removing its own directory from `PATH`):
   - `wc`: run the real `wc "$@"`, then re-emit every leading integer field right-aligned in 8
     columns (`printf '%8d'`), keeping the rest of the line (BSD `wc -c <f` prints `5`).
   - `stat`: reject `-c` (`stat: illegal option -- c`, exit 1). Support `-f FORMAT FILE` by
     translating `%d`, `%i`, `%u` unchanged, `%z` to `%s`, `%m` to `%Y` and `%Lp` to `%a` (match
     `%Lp` before any shorter directive), and calling the real `stat -c`; any other directive
     exits 1. `tests/read-guard-session-ledgers.sh:50` and
     `tests/session-start-heartbeat-escaping.sh:48` fall back to `stat -f '%Lp'` once `-c` is
     rejected, so BSD mode needs `%Lp`. No flag: delegate.
   - `date`: reject `-d` and `--date`. Accept `-u` anywhere among the options (before or after
     `-j`, `-f` or `-r`). Support `-j [-u] -f FORMAT VALUE +OUTFMT` for formats built from
     `%Y %m %d %H %M %S %z` and literal characters: build an anchored regex from FORMAT (literals
     escaped, so `...%SZ` rejects `.123Z` exactly like BSD `strptime` with leftover text), parse VALUE,
     compute the epoch with the real `date -u -d "YYYY-MM-DD HH:MM:SS"` minus the `%z` offset,
     print per OUTFMT; a mismatch exits 1. Task 2's `+hh:mm` branch runs `date -j -f ... +%s` without `-u`,
     and `_read_ledger.sh:245` runs `date -u -r "$epoch" +%Y-%m-%dT%H:%M:%S`, so also support
     `[-u] -r EPOCH +FMT` through the real `date -u -d @EPOCH`. Other invocations
     (`date -u +%Y-%m-%dT%H:%M:%S.000Z`) delegate.
   - Before you finish, `rg 'stat -f'` and `rg 'date -[jur]'` across `loom-hooks/**/*.sh` (hooks
     and tests): every form found must be supported by its shim. Add the missing form rather
     than leaving a hook site unexercised.
   The repository copies need no executable bit (`mistakes/hooks-shell-portability.md`: "Repo hook
   scripts do not need the executable bit", and chmod is blocked under `loom-hooks/` in a stage).
   `tests/_bsd_path.sh` defines `bsd_shim_dir`: it copies the three shims with
   `install -m 0755` into a fresh `mktemp -d` and prints that directory; callers put it first on PATH
   and remove it.
6. **`run-all.sh`**: `source "$SCRIPT_DIR/_bsd_path.sh"`; when `LOOM_HOOK_TEST_BSD=1`, create the shim
   directory once, `trap 'rm -rf "$BSD_DIR"' EXIT`, print `BSD tool shims active`, and make `run_test`
   run `env -u ... PATH="$BSD_DIR:$PATH" bash "$script"` (build the extra argument in an array; guard
   with `if`, never a bare `cond && action` as a function's last statement, per the same knowledge
   file). Keep the existing `env -u LOOM_HOOK_PATH ...` list: `_read_discipline.sh` sets
   `PATH="${LOOM_HOOK_PATH:-$PATH}"`, which would swallow the shim directory.
7. **`subagent-stop-heartbeat-lock.sh`**: its line `[[ "$(wc -l <"$JOURNAL")" != "1" ]]` compares a
   raw `wc` string and fails under the padded shim. Do not edit that line (existing assertion lines
   are never edited): add, before it, a test-local function
   `wc() { command wc "$@" | tr -d '[:space:]'; }` with a comment that the test's own assertions must
   tolerate BSD padding (the hook runs in a separate `bash` process and never sees the function).
8. **Shell tests** in `subagent-stop-review-harvest.sh`. PR #25 added to this file a padded-`wc`
   shim (`$TMP/shim/wc`, first on PATH inside `run_stop`): keep it as is. It also added, before the
   final `echo "PASS"`, a block that deletes `starts.jsonl`, runs a `reviewer-orphan` stop and
   asserts `rg -q 'reviewer-orphan skipped: no unambiguous' "$SKIP_LOG"` with
   `SKIP_LOG=.../hook-skips.log`. Rewrite that block (the `SKIP_LOG` path, the `rm -f`, the `if`
   condition and the FAIL text) to assert the new row: `stop-skips.jsonl` holds exactly one row with
   `agent_id == "reviewer-orphan"`, `agent_type == "loom-code-reviewer"` and
   `reason == "no_unambiguous_start_row"`, and stdout and stderr are empty. Those assertion lines
   exist at your base, so this edit may raise a test-integrity event: it is a required consequence of
   the plan's Decision 6, name it in your report; the orchestrator disputes it (workers never
   dispute). Every other new test goes before the final `echo "PASS"` as new lines only (source
   `tests/_bsd_path.sh` and `tests/_path_without.sh`):
   - a worker-type stop with no start row writes no `stop-skips.jsonl` row (the reviewer case is the
     rewritten block above).
   - in a subshell with `PATH="$(bsd_shim_dir):$PATH"` and `source _lifecycle.sh`:
     `loom_lifecycle_epoch 2026-01-01T00:00:00.123Z` prints `1767225600`;
     `2026-01-01T02:00:00+02:00` prints `1767225600`; `loom_lifecycle_resolve_start` resolves a row
     built like `run_stop` builds it (the padded `wc` from the BSD shim, through PR #25's inline
     `tr`).
   - with `PATH="$(path_without sha256sum)"` (skip with an echo when `command -v shasum` fails),
     `printf abc | loom_lifecycle_sha256` starts with
     `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`.
9. **Rust BSD variants** (include the shim files with `include_str!("../../../loom-hooks/tests/bsd-shims/wc")`
   and the same for `stat` and `date`; write them 0755 into a fixture-owned directory, never the repo):
   - `tests/worker_evidence/setup.rs`: `install_bsd_tools(dir)`; change the fixture `date` shim's
     fall-through line to `PATH="${FIXTURE_DATE_PATH:-/usr/bin:/bin}" exec date "$@"`.
   - `tests/worker_evidence/support.rs`: `Fixture::new_bsd(label)` (refactor `new` into a private
     `with_mode(label, bsd)`); a `bsd_bin: Option<PathBuf>` field; `configure` puts it after `bin` on
     PATH and sets `FIXTURE_DATE_PATH="<bsd_bin>:/usr/bin:/bin"` when present. Keep the file under 400.
   - `tests/worker_evidence.rs`, new tests: `bsd_padded_wc_records_lifecycle_and_settles` (mirror
     `exact_success_records_lifecycle_and_settles` on `new_bsd`) and
     `bsd_teammate_idle_after_a_stop_appends_to_a_non_empty_journal` (a stop, then an idle: two
     records, the second `claude_teammate_idle`).
   - `tests/codex_evidence/fixture_runtime.rs`: `Fixture::enable_bsd_tools(&self)` writes the shims
     into `self.root/bsd-bin`; `configure_child` prepends that directory to PATH when it exists. No new
     struct field (`fixture.rs` is not yours). `tests/codex_evidence/happy_path.rs`, new test
     `bsd_tools_record_the_same_terminal_identity`, `#[serial]`, mirroring
     `completed_job_preserves_request_and_terminal_identity` after `enable_bsd_tools`.
10. **`ci.yml`** job `hook-syntax`: after "Parse every shell hook" add a step installing
    `ripgrep fd-find jq` (`sudo apt-get update && sudo apt-get install -y ...`, then
    `sudo ln -sf "$(command -v fdfind)" /usr/local/bin/fd`), then steps `bash loom-hooks/tests/run-all.sh`
    and `LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh`. Acceptance greps for the text
    `LOOM_HOOK_TEST_BSD=1` in the file.

## Patterns to copy

`tests/_path_without.sh` (helper style), `tests/subagent-stop-review-harvest.sh` `run_stop`
(row and payload construction), `tests/worker_evidence/setup.rs` `install_shims` and `write_exec`.
Do not copy the existing fixture `date` shim's `PATH=/usr/bin:/bin` fall-through into the BSD
variant: it would bypass the BSD shim.

## Traps (knowledge, quoted)

- `mistakes/hooks-shell-portability.md`: "`wc -c` output has leading spaces on BSD";
  "A Bash Function Ending in `cond && action` Aborts a `set -e` Script"; "Hook Tests Inherit the
  Live Session's Environment" (`LOOM_HOOK_PATH` splices the real PATH back in; tests unset it).
- BSD mode runs all 95 tests with the shims; a failure in a test you do not own is a real finding.
  Report it by name; do not edit that file or exempt it.
- Existing assertion lines in tests are never edited; add new lines and new tests. The one
  exception is the PR's `reviewer-orphan` block (Task 8), which asserts a log the plan replaces.

## The one check

Run once, after your edits: `env LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh 2>&1 | tail -40`
from the repository root (a shell suite; it needs no compiled crate). Report its tail, and the
output of the two `rg` form checks in Task 5.

## Report

Files changed; the check result; `_lifecycle.sh` final line count; the rewritten `reviewer-orphan`
assertion block in `subagent-stop-review-harvest.sh` (a likely test-integrity event); the result of
`rg -n 'hook-skips' loom-hooks` (must be empty); failures in tests you do not own; anything not done.
