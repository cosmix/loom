# Worker C: Harness Output Reads and the Unoutlined-Read Warning (W3, W7)

Tier: `loom-senior-software-engineer`, effort medium. Wave 2, after worker F. Read `common.md` first.

## Files you own

- `loom-hooks/worktree-file-guard.sh`
- `loom-hooks/_read_ledger.sh` (W7 and the `read-discipline` ledger call)
- `loom-hooks/_read_discipline.sh` (only the flag `_loom_read_discipline_verdict1` sets)
- `loom-hooks/read-guard.sh` and `loom-hooks/poll-guard.sh` (only the tool name each passes to the ledger call)
- `loom-hooks/tests/worktree-file-guard-harness-output.sh` (new)
- `loom/tests/read_gate_contracts.rs` and `loom/tests/read_gate_contracts/**` (the first-read assertions W7 reverses)
- `loom/tests/integration/hooks_read_guard_cases.rs`, `loom/tests/integration/hooks_read_guard_repeat.rs`, `loom/tests/integration/hooks_poll_guard_reads.rs`, `loom/tests/integration/hooks_poll_guard_read_forms.rs`, `loom/tests/integration/hooks_read_guard.rs` (only assertions of an UNOUTLINED first-read deny)

## Read first

- `loom-hooks/worktree-file-guard.sh` whole (365 lines). It gates Read, Write, Edit, MultiEdit, Glob, Grep and NotebookEdit (`:38`); Grep and Glob reach the allow helpers through `.tool_input.path` as `LEXICAL_PATH`. The allow helpers are `owned_by_uid` (151-157), `allow_background_output` (159-169), `allow_scratchpad` (174-193) and `allow_loom_scratch` (200-210), joined at 248-251; the parent-component rejection is 227-230.
- `loom-hooks/_read_ledger.sh:238-313`: `_loom_read_gate_live`, `_loom_read_denial_record`, `_loom_read_rule1_warn` (278-282) and `loom_read_deny_once` (291-313), the only place the first whole read is denied (`exit 2` at 312), for Read (`read-guard.sh:101`) and for Bash `cat`/`sed` (`poll-guard.sh:230`) alike.
- `loom-hooks/_read_discipline.sh:265-283`: `_loom_read_discipline_verdict1` builds `LOOM_RD_V1_MSG`, with the outline when `_loom_outline_covered_rows` returns rows and the `no outline is available` text otherwise.
- `doc/loom/knowledge/architecture/read-gate.md` section "Rule 1 Is Deny-Once": the gate's design, which W7 narrows.

## W3: two new allow helpers in worktree-file-guard.sh

Claude Code's harness output lives at exactly `~/.claude/projects/<slug>/<session-id>/tool-results/<file>` (all 422 such directories on this host, no nesting, no symlinks), and a subagent's results live under the PARENT session id, which is the payload `session_id`. `~/.claude/projects` is write-denied in the capsule, not read-denied (`loom/src/sandbox/control_surfaces.rs:67`).

`allow_tool_results <lexical>`:

- Read tool only.
- The payload `session_id` matches `^[A-Za-z0-9_-]+$`; `HOME` is absolute.
- Let `P` be `canonical_existing "$HOME/.claude/projects"` (so a symlinked HOME or `~/.claude` still works, as `is_skill_md_path` handles it). The canonical target matches `^P/[^/]+/<session_id>/tool-results/[^/]+$`.
- No `..` component; no component from `P` down is a symlink; the target is a regular file with one link (`-f`, not `-L`, link count 1); every directory from `P` down is owned by `id -u` (`owned_by_uid`). Mirror `loom/src/relay/output.rs:93-110`.

`allow_tmpdir <lexical>`:

- Read tool only. Writes, edits, Grep and Glob never use it (a Grep or Glob over a directory would recurse into other sessions).
- The root is the literal `/tmp/claude-$(id -u)`, or `/private/tmp/claude-$(id -u)` on macOS, derived from the uid. Never consult `$TMPDIR`: hooks inherit the daemon's environment, and the Bash tool exports `TMPDIR=/tmp/claude-<uid>` only inside each command (see `common.md`). This is how `allow_background_output` and `allow_scratchpad` already key.
- The target is a regular, single-link, uid-owned, non-symlink file whose canonical path is under the canonical root, with no symlink at any component below the root.
- Refuse `<root>/<x>/<uuid>/...` when `<uuid>` matches `^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$` and differs from the payload `session_id`: one session never reads another's scratchpad. Fixture directories such as `client`, `home`, `clone` or `repo` are not session ids.

Add both to the allow chain at 248. The parent-component rejection at 227-230 still runs first, unchanged.

## W7: an unoutlined first read becomes a warning

Only the UNOUTLINED case changes; the outlined first read keeps today's deny-once, because its message carries the outline as the sanctioned route.

1. In `_loom_read_discipline_verdict1`, also set a global `LOOM_RD_V1_OUTLINED=1` on the outline branch and `0` on the `no outline is available` branch.
2. In `loom_read_deny_once`, after `_loom_read_discipline_verdict1` and before `_loom_read_denial_record`, return through `loom_hook_note_warn "$LOOM_RD_V1_MSG"; return 0` when `LOOM_RD_V1_OUTLINED` is 0. Record no denial row, so a later read of the same file warns again and is never denied.
3. The outlined path is unchanged, except that it calls `loom_record_block read-discipline first-read "<tool>" "$path"` right before its `exit 2`, with `<tool>` `Read` from `read-guard.sh` and `Bash` from `poll-guard.sh` (set a global in each caller, or pass it through), and puts `[read-discipline/first-read]` at the start of its `⛔ BLOCKED:` line.
4. A read that carries `offset` or `limit` gets no message at all, as today. Keep the offset-injection protections.

## Ledger in worktree-file-guard.sh

Call `loom_record_block worktree-file-guard <rule> "$TOOL_NAME" "$FILE_PATH"` in `block_path` before it returns, and put `[worktree-file-guard/<rule>]` at the start of the `LOOM: BLOCKED` banner line (the message's first line is empty). Map every `block_path` call site to its rule id from `common.md`, including `symlink-leaf` (`:273`), `unresolvable` (`:279`), `capability-token` (`:347`) and `shared-write` (`:358`). The unparsable-payload exit at `:22-34` does not go through `block_path`: record it as `bad-payload` with tool `unknown` and an empty subject.

## Existing tests that change

`loom/tests/read_gate_contracts.rs` is a frozen contract file of an earlier plan (commit 495f843a). Its `Gate::live` runs with a missing `LOOM_BIN` (`read_gate_contracts/support.rs:166-185`), so every read in it is unoutlined, and these tests assert the first whole read is denied: lines 24-29, 35-42, 54-60, 83-98, 102-112, 225-232, 236-250 and 256-265. Change each to assert exit 0 with `LOOM_HOOK_WARN` and the `no outline is available` guidance, keep every other assertion, and give the outlined deny-once one test of its own (a fake `LOOM_BIN` whose `map --outline` prints rows; first read exit 2, repeat exit 0).

In the four integration files, change only the assertions of an unoutlined first-read deny, the same way; leave every outlined case alone. Report each changed assertion with its file:line and its old and new expectation. The four bash `read-guard-*.sh` tests run without a live session and need no change.

## New test: `worktree-file-guard-harness-output.sh`

Follow the `common.md` test-environment rule. Set up the `/tmp/claude-<uid>` cases exactly as `loom-hooks/tests/worktree-file-guard-scratchpad.sh:10-30` does, including its SKIP path when the root is read-only (macOS confined runs have no private `/tmp`).

- Allows:
  - Read of `$HOME/.claude/projects/p/<sid>/tool-results/b1.txt` (with `HOME` set to a temp dir)
  - the same with `HOME` a symlink to the temp dir
  - Read of `/tmp/claude-<uid>/x.log` with TMPDIR unset
- Blocks:
  - Read of another session id's tool-results file
  - Read of a `tool-results/` entry that is a symlink to `/etc/hostname`, and of a hard-linked entry
  - A `tool-results/` directory owned by another uid (SKIP when the test cannot create one)
  - Write and Edit to either location
  - Grep and Glob with `path=/tmp/claude-<uid>`
  - Read of `/tmp/claude-<uid>/../x`
  - Read of `/tmp/claude-<uid>/<proj>/<other-uuid>/scratchpad/x`
  - Read through a symlink at depth 1 under `/tmp/claude-<uid>`

## Done when

The new script passes under bash and `LOOM_HOOK_TEST_BSD=1`, run once.
