# C3: the commit doctrine and the hooks

Stage `daemon-owned-commits`, wave 1, tier sonnet. Read `../common.md` first (Decision 1). Line numbers
were read at `ff3fe947`; locate every edit by symbol or by the quoted text.

## Role and issue

Issue #22. Every surface that tells a session to run `git commit` must now say what `../common.md` calls
the doctrine sentence, and the hooks must enforce it: a main agent in a loom stage session may not run
`git commit`, a subagent may not run the new stage commit command, the relay hook must recognise it, and
the attribution scan must cover its message. You touch signal text, templates, one skill, one preamble,
five hook scripts and their tests. No daemon or CLI code is yours.

## Files owned and files to read

Own exactly (repository-relative): `loom/src/orchestrator/signals/helpers.rs`, `.../cache.rs`, `.../merge.rs`,
`.../format/helpers.rs`, `.../format/codex.rs`, `.../tests_commit_timing.rs`, `.../tests_doctrine.rs`,
`CLAUDE.md.template`, `AGENTS.md.template`, `skills/loom-orchestration/SKILL.md`,
`loom-hooks/_subagent-preamble.txt`, `loom-hooks/commit-filter.sh`, `loom-hooks/commit-guard.sh`,
`loom-hooks/post-tool-use.sh`, `loom-hooks/loom-relay.sh`, `loom-hooks/tests/commit-filter-session-git-commit.sh`
(new), `loom-hooks/tests/post-tool-use-commit-reminder-tokenized.sh`, `loom-hooks/tests/loom-relay-kinds.sh`,
`loom-hooks/tests/run-all.sh`, `loom/tests/integration/hooks_commit_filter.rs`. (`.../` is
`loom/src/orchestrator/signals`.)

Read: `helpers.rs:121-143` (`append_commit_timing_rules`), `:166` (`STAGE_EXIT_RULES`, pinned, leave it);
`cache.rs:3,104,152,233,251-252,279-281,428-430`; `merge.rs:190-205` (`format_merge_task`);
`format/helpers.rs:237-242` (`append_stage_end_sequence`); `format/codex.rs:206-212`;
`tests_commit_timing.rs` (whole); `tests_merge.rs:66-92,170-190` and `knowledge.rs:235-253` (assertions you
must keep true); `CLAUDE.md.template:74-91` (Rule 4); `skills/loom-orchestration/SKILL.md:46-60`;
`commit-filter.sh:270-340,374-394`; `commit-guard.sh:588-593`; `post-tool-use.sh:264-305`;
`loom-relay.sh:50-110`; `loom-hooks/_common.sh` `loom_tokens_cmd_argv` (`:777-815`); the three existing hook
tests you extend; the `commit-filter-quoted-payload` line of `loom-hooks/tests/run-all.sh`;
`loom/tests/integration/hooks_commit_filter.rs` (`run_hook`, `:37-60`).

## Pinned interfaces

The doctrine sentence, verbatim on every surface (`../common.md`): Commit with `git add <specific-files>`
then `loom stage commit <stage-id> -m "type(scope): description"`, and wait for it with `loom request status
<id> --wait 90`; never run `git commit`. The relay kind is `commit` (`RequestKind::Commit`, a control
kind): `loom-relay.sh` maps `stage:commit` to it. The contract signal's "Do not commit" line
(`signals/contract.rs:242`) is not yours and stays.

## Root cause and current behaviour

- `helpers.rs:142` ends the shared commit-timing block with "Then stage your files, commit (...), and run
  `loom stage complete <stage-id>`." All four stable prefixes carry it (callers at `cache.rs:104,152,233,279`).
- `cache.rs:251` and `:281` tell a knowledge session to `git add doc/loom/knowledge/` and `git commit`;
  `merge.rs:201` tells a resolver to `git add` then `git commit`; `format/helpers.rs:241` and
  `format/codex.rs:209` say "commit".
- `CLAUDE.md.template:82` (Rule 4 fence) and `skills/loom-orchestration/SKILL.md` Rule 4 instruct
  `git add ... && git commit`; `commit-guard.sh:591` prints the same checklist line.
- `commit-filter.sh` blocks `git commit` from a subagent only; a main agent in a stage session may run it.
- `loom-relay.sh:62-68` has no `stage:commit` case; `drop_control_kinds` (`:100`) does not list `commit`.
- `post-tool-use.sh:301-304` fires the post-commit reminder on `git commit` only.

## Tasks

1. **Shared text.** In `helpers.rs` add `pub(super) fn commit_command_rule(add_target: &str) -> String`
   returning the doctrine sentence with `{add_target}` where `<specific-files>` stands. Change
   `append_commit_timing_rules(content, gate, review)` to take `add_target: &str` and end with
   `Then {commit_command_rule(add_target)} One logical commit per concern (module, tests, wiring, docs). Then
   run`loom stage complete <stage-id>`.` The rendered text reads "Then commit with ..." (lower case after
   "Then"): the helper's sentence starts with a lower-case "commit" where it follows "Then", and a surface that
   starts a sentence with it capitalises the first letter. Callers
   (`cache.rs:104,152,233,279`) pass a short const, never a literal, so each call stays ONE line under rustfmt's
   100-column and 60-column-argument widths (a `"doc/loom/knowledge/"` literal makes the call 102 columns and
   wraps it): `const FILES: &str = "<specific-files>";` for `:104,152,233` and `const KNOWLEDGE: &str =
   "doc/loom/knowledge/";` for `:279`, declared beside `CODE_STAGE_GATE` (the call `:233` sits in
   `generate_knowledge_distill_stable_prefix`, ledgered at 74 lines, and `:279` in
   `generate_knowledge_stable_prefix`, ledgered at 66; neither may grow, and a wrapped call grows it).
   Reason: `knowledge.rs:249` asserts the knowledge signal never contains the
   literal `git add <specific-files>`, and `cache.rs:429-430` and `tests_merge.rs:88-89` need `git add` and
   `git commit` present (the sentence's "never run `git commit`" satisfies both).
2. **Knowledge prefix.** Replace `cache.rs:251-252` and `:281` with one-line calls to two new `pub(super)`
   functions in `helpers.rs`: `append_knowledge_commit_header` ("COMMITS REQUIRED - commit through
   `loom stage commit`, never `git commit`; your commits go directly to main" plus the "NO MERGING" bullet,
   both substrings pinned at `cache.rs:428-431`) and `append_knowledge_commit_bullet` (the commit bullet using
   `commit_command_rule("doc/loom/knowledge/")` and the message `docs(knowledge): populate codebase
   knowledge`). `cache.rs` is ledgered at 524 lines and must end SHORTER AFTER `cargo fmt` (which the main
   agent runs once after the wave and measures; your pre-format count is not the measure, so keep every
   edited line under 100 columns and every call within 60 columns of arguments): check `wc -l` and report
   the number.
3. **Merge signal** (`merge.rs:format_merge_task`). Step 1 keeps "If `git status` shows a merge already in
   progress in this worktree, continue it; do not start a new `git merge`" and says: "Otherwise merge the
   target WITHOUT committing: `git merge --no-commit --no-ff {target_branch}` (a bare `git merge
   {target_branch}` commits by itself and cannot be signed in this sandbox)", then "(stage the resolution;
   never run `git merge --continue`, which commits)". The substring `git merge main`
   is asserted by `tests_merge.rs:86,174`: keep it. Step 4 becomes the doctrine sentence with
   `<specific-files>`, then "The worktree must end clean with no merge in progress". Step 5 is unchanged.
   The failure section (`format_failure_section`, near the end of the file) ends with "Still merge the target
   branch into this worktree, rerun the acceptance criteria, commit, and run `--resolved`": name the command
   there too, `git merge --no-commit --no-ff <target>` (the function has no target parameter: add one or
   write `<target>`) and `loom stage commit`, instead of "commit". The merge signal must not tell a resolver
   to commit by any other route.
4. **Recap lines.** `format/helpers.rs:241`: "`loom stage commit` (orchestrator only, one logical commit per
   concern)" in place of "commit (...)". `format/codex.rs:209`: "then commit at the end of the stage through
   `loom stage commit`". Leave `STAGE_EXIT_RULES` and `BLOCK-F` text alone (byte-pinned).
5. **Templates and skill.** `CLAUDE.md.template` Rule 4: the fence becomes four lines (`git add
   <specific-files>`, `loom stage commit <stage-id> -m "feat(scope): <description>"`, `loom request status <id>
   --wait 90   # its own Bash call, after the commit command returns`, `loom stage complete <stage-id>   # from
   the worktree root`) and the next line `Stage`git add <specific-files>` only, never `-A` or `.`; never run
   `git commit`.` Hard stop 3 (line 11) keeps its wording. Keep the sentinels `ONLY as the final step of the
   stage` (exactly one copy), `never mid-stage` and `When to commit — at the END, after ALL verification, never
   before.` (`tests_commit_timing.rs:73-95`). Skill Rule 4: add the doctrine sentence after the three conditions. The skill's coordinator preamble line
   `NEVER run git commit, git add -A/., or loom stage complete - only the main agent does` gains "(the main
   agent commits with `loom stage commit`)" so a coordinator is not told the main agent also may not commit.
   `AGENTS.md.template` tells sessions to run no git at all (line 51) and mentions commits only outside a stage
   (line 84): read lines 45-60 and 80-90; edit only if a line tells a stage session to commit (expect no edit).
6. **Preamble.** `_subagent-preamble.txt`: after the `NEVER run loom stage complete` bullet add `- NEVER run
   loom stage commit - the main agent commits your work with it`. Do not touch BLOCK-A or BLOCK-D bullets.
7. **`loom-relay.sh`.** In `relay_kind_at` add `stage:commit) echo commit ;;` beside `stage:block`; add
   `commit` to the `drop_control_kinds` case list; one comment sentence that a `commit` line is admitted like
   the others and never swept.
8. **`commit-filter.sh`** (494 lines: it must not end longer; trim redundant comment lines in the touched
   regions, never logic). (a) `is_subagent_git_operation`: token path adds `|| loom_tokens_cmd_has_arg_pair
   'loom' 'stage' 'commit'`, fallback regex adds `loom[[:space:]]+stage[[:space:]]+commit`; the block text
   gains "- NEVER run `loom stage commit` - only the main agent commits". (b) New `is_stage_commit_command`
   (same pair test) and gate the attribution section on `is_git_commit_command || is_stage_commit_command`,
   so the `-m` message of the new command is scanned by the existing checks. (c) Before the attribution
   section, a block for the MAIN agent: when `LOOM_STAGE_ID` and `LOOM_SESSION_ID` are both non-empty,
   `LOOM_HOOK_CONTEXT` is not `1`, and the command RUNS `git commit`, print to stderr `BLOCKED: no git commit in
   a loom stage session.` plus the doctrine sentence, and `exit 2`. This block matches `commit` only as git's
   SUBCOMMAND: the first non-option word after git's global options (`-C <dir>`, `-c <k=v>`,
   `--git-dir=<dir>`, `--work-tree=<dir>`, `--no-pager`), read through `loom_tokens_cmd_argv` (`_common.sh`).
   Do NOT reuse `is_git_commit_command` for it: `loom_tokens_cmd_has_arg 'git' 'commit'` is true for any
   argument equal to `commit`, so `git cat-file commit HEAD` and `git log --grep commit` would be blocked
   in every session. In a session both exit 0, `git commit -m x` and `git -C . commit -m x` exit 2. Keep
   bash 3.2 compatibility and the token-based reading (no regex over the raw command except in the existing
   unterminated-quote fallback); if the subcommand walk cannot be expressed with `loom_tokens_cmd_argv`
   probes, report it instead of editing `_common.sh` (not in your row). If the file cannot stay at or under 494
   lines without a new sourced helper, STOP and report it: a new hook file must be registered in
   `loom/src/fs/permissions/constants.rs` (`LOOM_HOOKS`), which is in no worker's row.
9. **`commit-guard.sh`** (649 lines, no growth): line 591 becomes `1. You have uncommitted changes. Commit
   with git add <specific-files> then loom stage commit $STAGE_ID -m 'type(scope): description'; wait with
   loom request status <id> --wait 90; never run git commit.` with the same `\n` structure. The message
   interpolates `$STAGE_ID` (the variable the checklist's own first line already uses) instead of the
   `<stage-id>` placeholder; `<id>` stays literal (the request id is only known after the commit command).
10. **`post-tool-use.sh`**: the reminder condition becomes `loom_tokens_cmd_has_arg 'git' 'commit' ||
    loom_tokens_cmd_has_arg_pair 'loom' 'stage' 'commit'`; update the comment above it.
11. **Hook tests** (add cases; change no existing check). New `commit-filter-session-git-commit.sh`, modelled
    on `commit-filter-quoted-payload.sh:30-75` (`run_hook`, `plain_payload`, `subagent_payload`; pass
    `LOOM_STAGE_ID=s1 LOOM_SESSION_ID=sess` through `"$@"`): in a session, `git commit -m x` exits 2 and stderr
    names `loom stage commit` and `--wait 90`; `git -C . commit -m x` exits 2; `git cat-file commit HEAD` and
    `git log --grep commit` exit 0 (two new cases: `commit` there is an argument, not git's subcommand); the
    same `git commit -m x` without the two variables exits 0; `loom stage commit s1 -m "feat(x): y"` exits 0; its message with a `Co-Authored-By:
    Claude <noreply@anthropic.com>` line exits 2; a subagent (`LOOM_MAIN_AGENT_PID=$$`) running it exits 2 and
    stderr names `loom stage commit`; `loom memory note "never run git commit"` in a session exits 0. Extend
    `post-tool-use-commit-reminder-tokenized.sh`: `loom stage commit s1 -m wip` fires the reminder;
    `loom memory note "loom stage commit later"` and `loom stage list && loom log commit` do not. Extend
    `loom-relay-kinds.sh`: `loom stage commit s1 -m "feat(x): y"` and `cd loom && loom stage commit s1 -m x`
    give `commit`; the same with agent type `general-purpose` gives no `commit`; `echo "loom stage commit s1"`
    gives nothing. Register the new test in `run-all.sh` with a `run_test` line directly after the
    `commit-filter-quoted-payload` line (`run_test "commit-filter: quoted prose about git is allowed, real
    commits blocked" ...`), found by that text, never by a line number (`platform-portability` edited the
    file); touch nothing else there (it also added the `LOOM_HOOK_TEST_BSD=1` mode; the new tests must pass in
    both modes).
12. **`loom/tests/integration/hooks_commit_filter.rs`**: the new session block makes `git commit` exit 2
    whenever both `LOOM_STAGE_ID` and `LOOM_SESSION_ID` are set, and a stage session's own Bash environment
    carries both (acceptance runs drop every `LOOM_*`, so the difference only shows in-session). `run_hook`
    (`:37-60`) therefore gains `.env_remove("LOOM_STAGE_ID").env_remove("LOOM_SESSION_ID")` on its
    `Command::new("bash")` chain; no assertion line changes. Add one test, `blocks_git_commit_in_a_stage_session`
    (exact name, module `hooks_commit_filter`), that sets both variables and expects exit 2 for `git commit -m
    "Fix bug in parser"`. Reach it without duplicating the spawn: move `run_hook`'s body into
    `run_hook_with_env(hook_path, tool_name, command, env: &[(&str, &str)])` (it removes the two variables,
    then applies `env`) and let `run_hook` call it with `&[]`. Spawn only `bash` (the hook), never the loom
    binary (`binary_spawn_guard.rs`).

## Tests to write (Rust)

In `tests_commit_timing.rs` (new functions only; `tests_doctrine.rs` is 399 lines and stays untouched, and
`RETIRED_PHRASES` lives in `tests_doctrine_blocks.rs`, which is not yours: report that):
`commit_command_doctrine_is_on_every_surface` (all four generators, the template, the skill contain `loom stage
commit` and `--wait 90`), `knowledge_prefix_carries_the_doctrine_without_specific_files` (the knowledge
prefix has `git add doc/loom/knowledge/` and not `git add <specific-files>`), `retired_session_commit_wording_is_gone`
(`concat!`-split phrases: `Then stage your` + `files, commit`; `git add <specific-files> &&` + `git commit -m`;
`git add doc/loom/knowledge/ &&` + `git commit`; checked on the four generators, the template, the skill and,
through `include_str!("../../../../loom-hooks/commit-guard.sh")`, that hook), and
`merge_signal_merges_without_committing` (build the content with `format_merge_signal_content` as `tests_merge.rs:66-82` does; contains
`git merge --no-commit --no-ff main` and `loom stage commit`).

## Patterns to copy and not copy

Copy `tests_commit_timing.rs`'s `all_generators` and `retired_commit_wording_is_gone` for the new tests, and
the tokenized helpers (`loom_tokens_cmd_has_arg_pair`, used by `is_stage_complete_command`) for every new
match. Do not regex-match a raw command except in the existing unterminated-quote fallbacks.

## Traps

- Knowledge (`patterns/doctrine-cross-surface.md`): "Editing a doctrine block means `rg` for a distinctive
  phrase of the OLD wording across `loom/src`, `skills/`, `agents/`, `loom-hooks/` and `doc/loom/knowledge/`
  before committing; a green `tests_doctrine` proves the two pinned surfaces agree, not that the doctrine is
  consistent." Run that `rg` for `stage your files`, `git commit` and `&& git commit`.
- Knowledge (`patterns/hook-content-stripping.md`): hook commands are tokenized so prose in one quoted
  argument is never a command; new matches go through the `loom_tokens_*` helpers.
- Never edit an existing assertion line (test-integrity `TI-edit`). New tests and new lines only.
- Byte budgets: `CLAUDE.md.template` is 18,891 bytes of its 20,480 cap (1,589 free; spend at most 400);
  `generate_stable_prefix()` has an 8,192 cap in `tests_size.rs` and about 7,600 bytes today (spend at most
  190 per prefix: the new tail is about 180 bytes longer than the old one); the orchestration skill and the
  preamble have no cap.
- `skills/loom-usage/SKILL.md:290` (an operator resolving a merge by hand), `agents/loom-software-engineer.md:94`
  (a subagent rule) and `skills/loom-git-workflow/SKILL.md` are outside your row and stay correct as they are.

## The one check

`bash loom-hooks/tests/run-all.sh` once from the repository root, then stop. Do not run cargo; the crate does
not compile until all six workers return.

## Report format

Files changed with line counts for `cache.rs`, `commit-filter.sh`, `commit-guard.sh` (pre-format counts; the
main agent re-measures `cache.rs` after `cargo fmt`); the check result;
byte sizes of `CLAUDE.md.template` after your edit; every deviation, every file outside your row you needed.
