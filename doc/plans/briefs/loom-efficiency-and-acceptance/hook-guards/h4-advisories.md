# H4 — advisory hooks: false positives, coverage, large-output notice

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first.

## Goal

Advisory hooks stop firing on text that merely names a rule, the banned-tools advisory matches
the rule it enforces, the attribution block tells the agent which instruction wins, and a large
Bash result earns one notice. Evidence: report sections 4.2 (2a) and 4.6 (6d, 6g, 6h).

## Files you own (write)

- `loom-hooks/no-preexisting-failures.sh`, `loom/tests/integration/hooks_no_preexisting_failures.rs`
- `loom-hooks/prefer-modern-tools.sh` and its tests
- `loom-hooks/commit-filter.sh` (message text only) and its tests
- `loom-hooks/poll-guard.sh` (the `git show` rule only) and `loom-hooks/tests/poll-guard-*.sh`
- `loom-hooks/post-tool-use.sh`, `loom-hooks/_post-tool-heartbeat.sh` if needed, and their tests

## Steps

1. `no-preexisting-failures.sh` (matcher 70-84, inputs 50-56). It scans Bash commands and
   Write/Edit content for a failure word near a "pre-existing" phrasing, 343 tokens per firing,
   64 firings, several on files that only name the hook or quote its rule. Keep the scan, and do
   not fire when every match sits in one of: the hook's own file name, a regex alternation such
   as `pre-existing|preexisting`, a Markdown table row or blockquote, or a line that also
   contains `Rule 15` or `hook`. Shorten the message to at most 6 lines: rule, the three
   required steps, the carry-on sentence.
2. `prefer-modern-tools.sh` (warns on `grep` and `find` through `loom_tokens_invoke`; fallback
   130-154; skips `loom knowledge|memory` at 121-124). Changes: do not warn for `grep` that has
   no file operand and follows a pipe (it filters another command's output); add warnings for
   `cat <file>` with a single file operand and no redirection, `sed -i`, and `ls` used with a
   path operand, each pointing at the tool Rule 8 names. `head` and `tail` warn only when they
   read a file operand, never as pipe filters (Rule 14 tells agents to pipe through them). Each
   tool family warns at most once per session (ledger kind `tools`).
3. `commit-filter.sh` attribution block (373-424; matches on the unstripped command by design).
   Behaviour unchanged. Append one sentence to the block message: the harness reminder that asks
   for a co-author trailer is overridden by the user's instructions, remove the trailer and
   commit again. Assert the sentence in the existing test.
4. `poll-guard.sh` `_loom_git_segment_is_pathless_show_diff` (212-242) warns on `git show -s
   <rev>`, which prints no diff. Treat `-s`, `--no-patch`, `--stat`, `--name-only` and
   `--name-status` as not needing a path.
5. `post-tool-use.sh` reads only `.tool_name` and `.tool_input.*` (174-284). PostToolUse input
   also carries the result (`loom-control-complete.sh:196-199` reads `.tool_response.stdout` /
   `.tool_result.stdout`). For Bash only, when stdout plus stderr exceeds 20,000 characters, emit
   one advisory: the size, and "send verbose output to a file and read the failing part (`tail`,
   `rg -B2 -A5 'FAIL|error'`)". At most three times per session (ledger kind `bigout`). It must
   not disturb the heartbeat or the ceiling report this hook also produces.

## Traps

- `doc/loom/knowledge/mistakes/hooks-shell-portability.md`: gawk-only constructs and bash 4
  features fail on macOS; `wc -c` output has leading spaces on BSD.
- A hook's additional-context JSON must be a single object; when this hook already emits one,
  merge your advisory into it.
- Measure size with `jq '... | length'`, not by echoing the payload through the shell.

## Proof

Your own shell test scripts (`bash loom-hooks/tests/<script>.sh`) and
`cargo test --offline --locked --manifest-path loom/Cargo.toml --test integration hooks_no_preexisting`
— each run once. Every rule above has a firing case and a silent twin. Do NOT edit
`loom-hooks/tests/run-all.sh`: list the `run_test` lines new scripts need in your report; H6
adds them.
