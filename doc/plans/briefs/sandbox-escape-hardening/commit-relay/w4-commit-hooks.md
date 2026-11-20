# W4: the hooks that see `loom commit`

Stage `commit-relay`, wave 1, tier sonnet. Read `../common.md` first (decision D1). Line numbers
were read at `3fc28031` (unchanged at `2908339a`); locate edits by function name.

## Why

`loom commit` (W2) prints a `LOOM_RELAY_V1 kind=commit ...` line. The relay hook must hand it to
`loom hook relay` as an allowed kind for the main agent and never for a subagent. The subagent
commit filter must block `loom commit` from a subagent as it blocks `git commit`, and its
attribution checks must cover a `loom commit` message. The post-commit reminder fires for
`loom commit` too.

## Files you own

- `loom-hooks/loom-relay.sh`, `loom-hooks/commit-filter.sh`, `loom-hooks/post-tool-use.sh`,
  `loom-hooks/git-add-guard.sh`
- `loom-hooks/tests/loom-relay-kinds.sh`, `loom-hooks/tests/commit-filter-quoted-payload.sh`,
  `loom-hooks/tests/post-tool-use-commit-reminder-tokenized.sh`,
  `loom-hooks/tests/git-add-guard-quoting.sh`

`loom-hooks/git-pre-commit-hook.sh` stays as it is: it runs only for commits made with `git` in
the main checkout (checkout sessions and the operator); the daemon's D1 commit runs with
`core.hooksPath=/dev/null`.

All three test files are already registered in `loom-hooks/tests/run-all.sh`: add cases to them;
never edit `run-all.sh` (other stages add to it). `_common.sh` belongs to `host-git-integrity`:
use its helpers, never edit it.

## 1. `loom-relay.sh`

- `relay_kind_at`: add `commit:*) echo commit ;;` (before `handoff:*`). `sub` is the word after
  `loom`, so `loom commit -m x -- a.rs` gives `commit:-m`.
- `drop_control_kinds`: add `commit` to the control list (it mirrors `RequestKind::is_control`,
  which W1 extends).
- The fast path (`case "$INPUT_JSON" in *"LOOM_RELAY_V1 "* | ...`) needs no change: a `loom commit`
  output carries the line. Add one sentence to the comment above it saying a `commit` line is
  admitted the same way and is never swept, so its line must reach this hook.
- Header comment: list `commit` with the other kinds if the header enumerates them.

## 2. `commit-filter.sh`

- `is_subagent_git_operation`: token path adds `|| loom_tokens_cmd_argv 'loom' 1 'commit'`
  (`_common.sh:777-823`: argv[1] of a segment whose command word is `loom`, so
  `loom memory note commit` or a quoted brief mentioning `loom commit` never matches). The
  fallback regex adds `loom[[:space:]]+commit\b`.
- The subagent block message gains "- NEVER run `loom commit` - only the main agent commits".
- `is_git_commit_command` (the attribution check's gate): token path adds
  `|| loom_tokens_cmd_argv 'loom' 1 'commit'`; fallback regex adds the same `loom commit`
  pattern. The block text says "commit command" where it says "git commit command".
- Header comment: say the filter also covers `loom commit`, the D1 route a stage worktree session
  commits by.

## 3. `post-tool-use.sh`

The reminder block (`if loom_tokenize_command "$STRIPPED_COMMAND" && loom_tokens_cmd_has_arg 'git' 'commit'`)
fires when either `loom_tokens_cmd_has_arg 'git' 'commit'` or
`loom_tokens_cmd_argv 'loom' 1 'commit'` holds. Update the comment above it.

## 4. `git-add-guard.sh`

The block message (the heredoc after `if ! check_dangerous_patterns "$COMMAND"`) teaches
`git add <specific-files>` only. A stage worktree session can no longer run `git add` at all (D1),
so replace its `CORRECT PATTERN:` block with two cases, keeping every other line:

    CORRECT PATTERN:
      In a stage worktree session:
        loom commit -m "<type(scope): description>" -- <specific-files>
        (then run: loom request status <id> --wait 90, as its own Bash call)
      In the main checkout (a knowledge stage, a merge resolution):
        git add <specific-files>

The detection logic does not change.

## Tests (add cases, change no existing check)

- `git-add-guard-quoting.sh`: one added case: a blocked `git add -A` prints `loom commit` on
  stderr.

- `loom-relay-kinds.sh`: with `allowed_for`: `loom commit -m "feat(x): y" -- a.rs` gives
  `commit`; `cd loom && loom commit -m x -- src/a.rs` gives `commit`; the same command with
  agent type `general-purpose` gives no `commit` (mirror the existing subagent control-kind
  case); `echo "loom commit -m x"` gives nothing.
- `commit-filter-quoted-payload.sh`: a subagent payload (mirror the existing subagent cases:
  `LOOM_MAIN_AGENT_PID=$$` plus `agent_type`) running `loom commit -m x -- a.rs` exits 2 and the
  stderr names `loom commit`; the main agent's `loom commit -m x -- a.rs` exits 0; a subagent's
  `loom memory note "never run loom commit"` exits 0; the main agent's
  `loom commit -m $'feat: x\n\nCo-Authored-By: Claude <noreply@anthropic.com>' -- a.rs` exits 2.
- `post-tool-use-commit-reminder-tokenized.sh`: (d) `loom commit -m wip -- a.rs` fires the
  reminder; (e) `loom memory note "loom commit later"` does not.

## Knowledge you need

Hook commands are tokenized (`loom_tokenize_command`) so prose in one quoted argument is never a
command (`patterns/hook-content-stripping.md`); every new match goes through the `loom_tokens_*`
helpers, with a regex only in the existing unterminated-quote fallbacks.

## Check

One run: `bash loom-hooks/tests/run-all.sh` from the repository root. It passed 95 of 95 at
`2908339a` on the host.
