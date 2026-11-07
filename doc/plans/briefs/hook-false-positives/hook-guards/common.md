# Shared Context: hook-guards

Every worker reads this file before its own brief. The plan, `doc/plans/PLAN-hook-false-positives.md`, holds the evidence and the decisions. This file pins the interfaces workers share.

## Rules for every hook edit

- Bash 3.2 compatible (the BSD suite runs `LOOM_HOOK_TEST_BSD=1`): no associative arrays, no `${arr[-1]}`, no `declare -n`, no `mapfile`, no `realpath`, no `readlink -f`; guard empty arrays under `set -u` the way `check_cross_worktree_tokens` does (`loom-hooks/worktree-isolation.sh:251-267`). Knowledge: `mistakes/hooks-shell-portability.md`.
- No new files under `loom-hooks/` other than test scripts in `loom-hooks/tests/`. New helper functions go into files the guards already source: `_containment.sh` and `_common.sh` (worker F), `_read_ledger.sh` (worker C), `_git-command.sh` (worker D; already sourced at `commit-filter.sh:39` and embedded at `loom/src/fs/permissions/constants.rs:17,203`). Reason: `constants.rs` (`LOOM_HOOKS`, line 198) embeds every helper file, and 17 Rust test fixtures copy fixed helper lists (`rg -l '"_common.sh"' loom/src loom/tests`), so a new sourced file breaks them.
- `_common.sh:72-79` sources `_parser.sh`, `_parser_expand.sh`, `_parser_scan.sh`, `_identity.sh`, `_stage_lookup.sh`, `_containment.sh` and `_heartbeat.sh`. All seven guards this plan edits already source `_common.sh` unguarded (`worktree-isolation.sh:59`, `loom-control-complete.sh:13`, `worktree-file-guard.sh:13`, `subagent-verify-guard.sh:50`, `commit-filter.sh:38`, `git-add-guard.sh:53`, `read-guard.sh:26`), so each sees `loom_tokenize_command`, `loom_payload_agent_verdict` and the functions below. Add no conditional sourcing.
- A guard that cannot decide fails closed exactly as it does today, unless your brief names a warn path.
- Files already over 400 lines (`subagent-verify-guard.sh` 591, `commit-filter.sh` 454) take only the minimal rule edits, no net growth beyond about 15 lines each. Worker D puts the W6 decision helper in `_git-command.sh`. Do not split either file in this stage.
- Anchor every edit by symbol (`rg -n '<function>' <file>`). Line numbers in the briefs are as observed at commit 104ec387 and drift once a sibling plan merges.
- Hook tests live in `loom-hooks/tests/<name>.sh`. Add no `run_test` lines to `loom-hooks/tests/run-all.sh`: the orchestrator adds them after every worker returns. List each new test script in your report.
- Test environment. Hooks read TMPDIR and LOOM_SCRATCH_DIR, and `run-all.sh:31-35` passes the caller's TMPDIR through, so every hook invocation in a test runs as `env -u LOOM_SCRATCH_DIR TMPDIR="$TMP/tmpdir" ...`, where `$TMP/tmpdir` is created uid-owned and holds no fixture. Set `LOOM_SCRATCH_DIR` only in the cases that test it, to a uid-owned mode-700 directory outside `$TMP/repo`. Every block target in a test resolves outside `$TMP/tmpdir` and outside that scratch dir. Path resolution is physical (interface 1), so create every directory a case `cd`s into or names (`web/src`, `a/b`, `.worktrees/<sibling>`), and give each fixture worktree the real `<wt>/.loom/work -> ../../../.loom/work` symlink that `ensure_work_symlink` creates (`loom/src/git/worktree/settings.rs:49-76`).
- Checks: run at most one narrowly-scoped check, once: your own new bash test script (`bash loom-hooks/tests/<your-test>.sh`, plus its `LOOM_HOOK_TEST_BSD=1` run when your brief says so), or one filtered Rust test. `bash -n <file>` is always fine. The guards that run during THIS stage are the installed HEAD copies, which still block a subagent's `cargo check`, `cargo build` and `cargo fmt`: compile Rust only through your one filtered `cargo test` run.

## Pinned interface 1: effective working directory (owner: worker F, file `loom-hooks/_containment.sh`)

```bash
# loom_track_cwd [<start-dir>] - after a successful loom_tokenize_command, fill
# the global array LOOM_TOKEN_CWD so LOOM_TOKEN_CWD[i] is the absolute physical
# working directory token i runs in, or "" when it cannot be known. Starts from
# <start-dir> made physical when it is an absolute existing directory (callers
# pass the payload's .cwd: jq -r '.cwd // empty' <<<"$INPUT_JSON"), else from
# `pwd -P`. Also sets the global LOOM_SCRATCH_EXPAND to 1 or 0 (rule below).
loom_track_cwd "$payload_cwd"

# loom_normalize_path <base> <path> - print the absolute physical path <path>
# names when run from <base>. A relative <path> joins <base>; `~` and `~/x`
# expand to $HOME; a leading `$LOOM_SCRATCH_DIR` or `${LOOM_SCRATCH_DIR}`
# expands to the hook's own LOOM_SCRATCH_DIR, only while LOOM_SCRATCH_EXPAND=1
# and LOOM_SCRATCH_DIR passes the scratch test below.
# Walk the components left to right: drop empty and `.` components; before a
# `..` is applied, replace the prefix built so far with
# `(cd -P -- "$prefix" && pwd -P)` when it is an existing directory, then pop
# one component (never above `/`). After the walk, make the longest existing
# directory prefix physical the same way; a missing tail stays lexical.
# Returns 1 for `~user`, for any other `$` or a backtick, for an empty <base>
# with a relative <path>, and when `cd -P` fails on an existing prefix.
loom_normalize_path "$base" "$path"

# loom_path_class <abs-physical-path> - print exactly one of, tested in this
# order (the first match wins):
#   state     under the main checkout's .loom/work or .work, or under
#             <repo>/.git (the common git dir)
#   sibling   under <repo>/.worktrees/<other-id>
#   worktree  under the current worktree root (loom_current_worktree)
#   scratch   under LOOM_SCRATCH_DIR, when it is absolute, an existing
#             directory, not a symlink, and owned by `id -u`
#   outside   anything else, the main checkout's own files included
loom_path_class "$abs"
```

Every root is compared in physical form (`cd -P -- "$root" && pwd -P`), and every prefix test ends at a path boundary, so `/a/wt-x` is not under `/a/wt`. The repo root is the parent of `.worktrees/`. TMPDIR is never a scratch root: hook processes inherit the daemon's environment (`loom/src/process/environment.rs:94`, `STAGE_HOST_ENV_ALLOWLIST`), not the Bash tool's per-command `TMPDIR=/tmp/claude-<uid>`; the hook read ledgers under `/tmp/loom-reads/<sid>` (the `${TMPDIR:-/tmp}` fallback) prove it on this host. In production LOOM_SCRATCH_DIR is `$XDG_RUNTIME_DIR/loom/scratch/<sid>`, else `~/.cache/loom/scratch/<sid>` (macOS `~/Library/Caches/loom/scratch/<sid>`), a real uid-owned 0700 directory never under `/tmp` or the repo (`loom/src/relay/scratch.rs:33-60`), with the same value in the hook and the Bash environment (`loom/src/orchestrator/terminal/native/wrapper/host_env.rs:46-51`).

Rules for `loom_track_cwd`. A segment is the run of tokens between two `%%SEP%%` sentinels; the separator characters of a sentinel are in `LOOM_TOKEN_SEPS[$i]` (`loom-hooks/_parser.sh:113-117`). The tokenizer gives `$(` no character (its sentinel reads only the separator before it) and leaves an empty entry at an `sh -c` splice; runs merge (`;\n`, `&&(`, `);`).

- Taint: when any sentinel's SEPS contains `(`, `)` or a backtick, or is empty, every segment after the first `cd`, `pushd` or `popd` segment gets `""`. A subshell, a command substitution or an `sh -c` body can change directory without changing the outer shell's.
- A `cd <word>` segment (exactly two word tokens) whose target normalizes against the current cwd:
  - followed by a sentinel whose SEPS is exactly `&&`: later segments get the target. When the target is not an existing directory at hook time, mark the cd unverified.
  - followed by a sentinel whose SEPS holds only `;` and newline characters: later segments get the target only when it is an existing directory at hook time, else `""`.
  - followed by anything else (`|`, `||`, `&`, any merged run with other characters): later segments get `""`.
- After an unverified cd, the first sentinel whose SEPS is not exactly `&&` makes every later segment `""`: a failed cd stops an `&&` chain, and a `;` resumes in the old directory.
- `cd` with no argument, `cd -`, `cd` with an option or a third token, a target that does not normalize, `pushd` and `popd`: every later segment gets `""`.
- Every token of a segment gets that segment's cwd; sentinel indices get `""`.

Rule for `LOOM_SCRATCH_EXPAND`: 1 unless some token contains the text `LOOM_SCRATCH_DIR` not directly preceded by `$` or `${`, or some word token is exactly one of `eval`, `env`, `declare`, `typeset`, `export`, `local`, `readonly`, `read`, `unset`, `set`, `source`, `.`, `printf`, `mapfile`, `readarray`. A command that could reassign the variable never gets the expansion.

## Pinned interface 2: block ledger (owner: worker F, file `loom-hooks/_common.sh`)

```bash
# loom_record_block <guard> <rule> <tool> <subject> - append one JSON line to
# "$LOOM_WORK_DIR/heartbeat/$LOOM_STAGE_ID.blocks.jsonl" when LOOM_WORK_DIR is
# absolute and an existing directory, LOOM_STAGE_ID and LOOM_SESSION_ID are
# set, LOOM_STAGE_ID matches ^[A-Za-z0-9_-]+$, LOOM_CODEX_FORWARD is empty,
# and the heartbeat directory exists as a directory (not a symlink) or can be
# created mode 700. Never prints, never changes the caller's exit status
# (always returns 0), and needs jq (already required by every guard).
loom_record_block "worktree-isolation" "path-traversal" "Bash" "$COMMAND"
```

One line per call, built with `jq -nc --arg`, appended with a single `printf '%s\n' "$line" >>"$file" 2>/dev/null`; `ts` comes from `date -u +%Y-%m-%dT%H:%M:%SZ`. Ledger file safety, checked immediately before the append (the same tests `loom_lifecycle_journal_ready` in `loom-hooks/_lifecycle.sh` makes; copy them inline, because `_common.sh` does not source `_lifecycle.sh` and the shared-helper decision forbids new sourcing):

- `$LOOM_WORK_DIR/heartbeat` must be a directory and not a symlink, and the ledger path must be absent, or a regular file that is not a symlink (`[[ ! -L "$f" && ( -f "$f" || ! -e "$f" ) ]]`). A FIFO, device or symlink at the ledger path writes nothing, so a planted FIFO can never hang the guard before its exit 2.
- An existing ledger over 4 MiB (`wc -c`) writes nothing: the ledger is diagnostics and stops growing instead of being rotated.
- The append runs under `umask 077` in a subshell, so a new ledger is mode 600.
- No lock: each line is one `printf` of at most about 2 KiB to an `O_APPEND` file, which concurrent hooks on a local filesystem append whole.

`common-block-ledger.sh` proves each refusal (FIFO under `timeout 5`, symlinked ledger, a ledger over 4 MiB) and the new file's mode 600.

```json
{"ts":"2026-11-07T12:00:00Z","stage":"<id>","session":"<id>","role":"main|subagent","guard":"<guard>","rule":"<rule>","tool":"<tool>","subject":"<first 300 bytes>"}
```

- `role` is `subagent` when `loom_payload_agent_verdict "$INPUT_JSON"` (`_identity.sh:255`) prints `subagent`, else `main`. `INPUT_JSON` is the caller's global; empty is allowed.
- `subject` is cut to at most 300 BYTES under `LC_ALL=C`, then the final UTF-8 sequence is dropped when the cut left it incomplete (a trailing lead byte, or trailing continuation bytes fewer than their lead byte announces). The result is always valid UTF-8 when the input was.
- Codex lane: hooks under the forwarded codex job (`LOOM_CODEX_FORWARD` non-empty, exported at `codex-forward.sh:7`) run inside the stage Bash sandbox, where `<repo>/.loom` is denied (`loom/src/sandbox/control_surfaces/session_denies.rs:79-80`), so `loom_record_block` returns 0 at once there. Codex blocks are out of the ledger's scope by decision.
- The heartbeat directory is where Claude-lane hooks already write session state (`loom-hooks/_post-tool-heartbeat.sh:128-136`; `post-tool-use.sh` writes `heartbeat/<stage>.<sid>.context-ceilings` today). Every Rust reader addresses `<stage>.json` by name or filters on the `json` extension (`loom/src/orchestrator/monitor/heartbeat.rs:213-221`, `heartbeat_store.rs:57`, `status/data/heartbeat_facts.rs:35-41`), no watcher reacts to writes there, and nothing but `loom init`/`loom clean` deletes the directory, so `*.blocks.jsonl` is invisible to the orchestrator and lives for one plan run.

## Guard and rule ids

Every block path calls `loom_record_block` immediately before it exits 2. The FIRST NON-EMPTY line of its stderr message starts with `[<guard>/<rule>]`: several banners open with a blank line or a heredoc (each `print_*` heredoc in `worktree-isolation.sh`, the `LOOM: BLOCKED` banner of `worktree-file-guard.sh` `block_path`, `git-add-guard.sh:296`), so the prefix goes on the banner line, not on an empty first line.

| Guard | Rule ids |
| --- | --- |
| `worktree-isolation` | `git-override`, `path-traversal`, `cross-worktree`, `policy-load` (the exits at `:96-97`). The file has one `exit 2` (`:347`) after the `print_*` calls: set a rule-id global at each call site and record before that exit. |
| `loom-control-complete` | `not-pinned` |
| `worktree-file-guard` | `outside-worktree`, `parent-component`, `control-chars`, `missing-path`, `tilde`, `symlink-leaf` (`:273`), `unresolvable` (`:279`), `capability-token` (`:347`), `shared-write` (`:358`), `bad-payload` (the unparsable-payload exit at `:22-34`, which bypasses `block_path`) |
| `subagent-verify-guard` | `subagent-verify`, `gate-once` |
| `commit-filter` | `subagent-git`, `no-git-commit`, `isolation-bypass`, `subagent-complete` (`:325`), `ai-attribution` (`:448`), and any other block path in the file, named after its reason |
| `git-add-guard` | `dangerous-add` |
| `read-discipline` | `first-read` (the outlined first-read deny that stays in `loom_read_deny_once`, `_read_ledger.sh:291-313`; tool `Read` from `read-guard.sh`, `Bash` from `poll-guard.sh`) |

Not recorded, by decision (knowledge-distill files them as a concern): `commit-guard.sh`, `credential-guard.sh`, `spawn-guard.sh`, `plans-path-guard.sh`, `stage-terminal-guard.sh`, `codex-forward-guard.sh`, and `poll-guard.sh`'s `loom_hook_deny_or_warn` (`_read_discipline.sh:215-222`). Also not recorded: the infrastructure exits that precede any decision, namely `loom_require_jq`'s missing-jq exit (`_common.sh:30-34`; the ledger itself needs jq) and a guard's failure to source its helpers. The ledger promises one line per semantic block of the seven guards in the table above, nothing wider.

## The new BLOCK-A text (owners: worker D for the guard message, codex J2 for every other copy)

Copied byte for byte into `loom-hooks/subagent-verify-guard.sh` (the subagent message, after its `⛔ BLOCKED:` framing line), `loom-hooks/_subagent-preamble.txt`, `loom/src/orchestrator/signals/cache/blocks.rs` and `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` (`BLOCK_A`). `loom/src/orchestrator/signals/tests_doctrine.rs` compares the copies (the guard through `include_str!` at `:122`), so it passes only once worker D's and J2's copies are both in the tree.

`cargo fmt -- <files>` is NOT a per-file form: cargo passes every target's files to rustfmt and appends the named ones, so it reformats the whole package (observed on a scratch crate). `rustfmt --edition 2021 <files>` touches only the named files plus the child modules a named file declares with `mod x;`, and the HEAD guard already allows it.

```text
VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:
- Compile the files YOU wrote as often as you need (`cargo check`,
  `cargo build`, `tsc --noEmit`, `bun run build`) and format them with
  `rustfmt --edition 2021 <files>`. No full test suite, no linter, no
  `cargo fmt`, and never a repeated or looping check.
- AT MOST ONE narrowly-scoped check over the files YOU wrote (e.g.
  `cargo test <your_module>::`), run ONCE. Skip it if you are unsure.
- Report instead: files changed, assumptions made, anything unresolved.
  The stage's gate compiles, tests, and lints; the MAIN AGENT fixes.
```

## Memory

Record mistakes, decisions and surprises with `loom memory note` / `loom memory decision` the moment they happen. Never use `loom knowledge` here, and never Claude Code auto-memory.
