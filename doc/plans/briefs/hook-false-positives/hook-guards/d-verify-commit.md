# Worker D: Subagent Compile Commands and Scratch-Repo Git (W5, W6)

Tier: `loom-senior-software-engineer`, effort medium. Wave 2, after worker F. Read `common.md` first.

## Files you own

- `loom-hooks/subagent-verify-guard.sh`
- `loom-hooks/commit-filter.sh`
- `loom-hooks/_git-command.sh` (the W6 helper; 29 lines today, sourced at `commit-filter.sh:39` and `post-tool-use.sh:254`)
- `loom-hooks/git-add-guard.sh` (ledger call and message prefix only)
- `loom/tests/integration/hooks_subagent_verify_guard_cases.rs`
- `loom/tests/integration/hooks_subagent_verify_guard_main.rs`
- `loom/tests/integration/hooks_subagent_verify_guard.rs`
- `loom-hooks/tests/commit-filter-scratch-repo.sh` (new)

## Read first

- `loom-hooks/subagent-verify-guard.sh` lines 1-200 and 290-400: role detection (100-135), the comment naming the BLOCK-A copies (167-173), `block()` (174-194), the whitespace split it tokenizes with (`TOKENS=($STRIPPED)`, `:163`; `--` is its own token), `check_cargo` (295-), `check_tsc` (331-), `check_node_pm` (371-), and the `npx`/`bunx` prefix skip (`:571`; `node_modules/.bin/tsc` maps to `tsc` through `##*/`).
- `loom/tests/integration/hooks_subagent_verify_guard_cases.rs` module docs before editing a case; `hooks_subagent_verify_guard.rs:243-286` runs the table.
- `loom-hooks/commit-filter.sh`: tokenize at `:84`, the isolation-bypass rule (161-225), `is_subagent_git_operation` (246-262), the subagent git rule (284-306) with the subagent `loom stage complete` deny inside it (308-325), the no-git-commit rule (333-345), the AI-attribution block (`:448`).
- `loom-hooks/_git-command.sh` whole: `loom_git_runs_commit` already walks git's global options.

## W5: what changes in subagent-verify-guard.sh

For a SUBAGENT:

- Allow `cargo check` and `cargo build` with any arguments: in `check_cargo`, `build | check` no longer block. `clippy` and every `cargo fmt` form still block. `cargo fmt -- <files>` is not a per-file form (it reformats the whole package; see `common.md`), and `rustfmt --edition 2021 <files>` is already allowed at HEAD and stays so.
- Allow `cargo test` with `--no-run` for both roles (it is a compile, like `cargo build --tests`).
- Allow `tsc` whenever `--noEmit` is present and neither `-w` nor `--watch` is, in every invocation form the guard recognizes (`tsc`, `npx tsc`, `bunx tsc`, `node_modules/.bin/tsc`). A `tsc` that emits still blocks.
- Allow `<pm> run build` for bun, npm, pnpm and yarn. `vite build` is already allowed at HEAD (no vite case exists). `lint` and `typecheck` scripts still block, whatever they run: the guard does not inspect a script's body (in `web/package.json`, `typecheck` is `tsc -b --noEmit` and `build` is `tsc -b && vite build`). `go build`, `make build` and `make check` stay blocked.
- Test runners keep today's rules (unfiltered suites block, scoped runs pass).

Replace the subagent block message body with the BLOCK-A text from `common.md`, byte for byte, after the `⛔ BLOCKED: Subagent attempting project-wide verification.` line and its blank line. Put `[subagent-verify-guard/subagent-verify]` at the start of that framing line and `[subagent-verify-guard/gate-once]` at the start of the main-agent line. Codex J2 changes every other BLOCK-A copy, and `loom/src/orchestrator/signals/tests_doctrine.rs` compares them at the gate. Update the comment at 167-173 so it names the four real copies: this guard, `loom-hooks/_subagent-preamble.txt`, `loom/src/orchestrator/signals/cache/blocks.rs` and `tests_doctrine_blocks.rs` (`CLAUDE.md.template` holds none).

Call `loom_record_block subagent-verify-guard <rule> Bash "$COMMAND"` in `block()` before each `exit 2`.

Case table:

- Move only the two cases now allowed from `BLOCK_CASES` to `ALLOW_CASES`: `cargo build` (`:22`) and `npm run build` (`:44`).
- The REGRESSION entries that use `cargo build` as their vehicle (`:101-104`, `:105-108`, `:125-128`) test the separator handling, not cargo: keep them in `BLOCK_CASES` and change `cargo build` to `cargo clippy`. Do the same for the ALLOW regression at `:213-216` (`rg "x && cargo build -v" src/` becomes `rg "x && cargo clippy -v" src/`) and for `subagent_git_commit_message_with_internal_separator_allowed` in `hooks_subagent_verify_guard.rs:358-371`, whose comment says it depends on `cargo build` blocking. Without that change each regression passes whatever the tokenizer does.
- Add block cases: `cargo fmt --all`, `cargo fmt -- src/a.rs`, `cargo fmt --check`, `tsc -w`, `tsc --noEmit -w`, `bun run lint`, `bun run typecheck`. (`cargo fmt` and `cargo clippy` exist already at `:23-24`.)
- Add allow cases: `cargo check --lib --tests`, `cargo test --lib --no-run`, `tsc --noEmit`, `tsc -b --noEmit`, `bunx tsc --noEmit -p .`, `node_modules/.bin/tsc --noEmit`, `bun run build`.
- In `hooks_subagent_verify_guard_main.rs`, add `cargo test --lib --no-run` as allowed for the main agent in a standard stage.

## W6: scratch repositories in commit-filter.sh

Add one helper to `_git-command.sh`, `loom_git_targets_scratch_only`, and call it from `commit-filter.sh` so that ONLY the `git commit` and `git add` terms of `is_subagent_git_operation` and the no-git-commit rule are lifted when it returns 0. `loom stage commit`, `indirection_intent`, the subagent `loom stage complete` deny (308-325), the isolation-bypass rule (unsetting `LOOM_MAIN_AGENT_PID` and friends, 161-225), the AI-attribution block and `git-add-guard.sh` (`git add -A` / `git add .` stay blocked even in a scratch repo) always apply.

`loom_git_targets_scratch_only` returns 0 only when all of these hold; anything else is no exemption (today's behavior):

1. `TOKENS_OK` is 1, and `loom_track_cwd "$(jq -r '.cwd // empty' <<<"$INPUT_JSON")"` ran.
2. The command has at least one git segment. A command with no git segment is never exempt (otherwise `cd <scratch>/fx && loom stage complete <id>` would pass).
3. No git segment carries `--git-dir`, `--work-tree`, `-c`, `--namespace`, `--config-env`, more than one `-C`, or a `GIT_*=` assignment, and no token is `env` or starts with `GIT_`.
4. For each git segment, the repo dir is its `-C` argument normalized against its cwd, else its cwd; it is never `""`, and its class is `scratch`.
5. For each distinct repo dir, `env -u GIT_DIR -u GIT_WORK_TREE -u GIT_COMMON_DIR git -C <dir> rev-parse --show-toplevel --absolute-git-dir --git-common-dir`, run through `loom_run_bounded 2` (`_bounded.sh`, already sourced at `commit-filter.sh:46`), prints three paths, and the physical form of each is class `scratch`. Checking the git dirs too defeats a `.git` gitfile in scratch that points into the main repository (observed: `S/fx/.git` holding `gitdir: R/.git/worktrees/W` made rev-parse print `S/fx` as the toplevel while `git commit` wrote to repo R).
6. Same-command init: when rev-parse fails because no repository exists at or above the dir's nearest existing ancestor, the dir still counts when that ancestor is class `scratch` and an earlier git segment of the same `&&` chain is `git init` with no argument and the same cwd, or `git init <arg>` whose argument normalizes to the dir. This covers the evidence form `mkdir -p <S>/fx2 && cd <S>/fx2 && git init -q && git add doc && git commit -qm x` (F's tracker gives the git segments `<S>/fx2` through the `&&` chain).

Keep the change in `commit-filter.sh` to the call and its condition; the helper lives in `_git-command.sh`.

Ledger and message prefixes: every block path in `commit-filter.sh` and `git-add-guard.sh` calls `loom_record_block` with the guard name and the rule id from `common.md` (`subagent-git`, `no-git-commit`, `isolation-bypass`, `subagent-complete`, `ai-attribution`, `dangerous-add`), and puts `[<guard>/<rule>]` at the start of its message's first non-empty line (`git-add-guard.sh:296` opens a heredoc with an empty line).

## New test: `commit-filter-scratch-repo.sh`

Follow the `common.md` test-environment rule. Run the hook as a stage session (`LOOM_STAGE_ID`, `LOOM_SESSION_ID` set), for the subagent cases with a subagent payload and `LOOM_MAIN_AGENT_PID=$$` the way `loom-hooks/tests/commit-filter-session-git-commit.sh:23,56,90-107` does (it scrubs the ambient `LOOM_*` first), with `LOOM_SCRATCH_DIR` a uid-owned mode-700 temp dir outside the fixture repo. Without the stage variables and the live chain the rules never fire, and every case passes at HEAD.

- Allowed for both a subagent and the main agent:
  - `cd <S>/fx && git add doc && git commit -qm x`, with `fx` an existing repo, the path written literally
  - `cd "$LOOM_SCRATCH_DIR/fx" && git add doc && git commit -qm x`
  - `mkdir -p <S>/fx2 && cd <S>/fx2 && git init -q && git add doc && git commit -qm x`
- Still blocked:
  - the same commands with the cwd in the worktree
  - `cd "$X" && git commit -qm x`
  - `unset LOOM_MAIN_AGENT_PID; git status`
  - `cd <S>/fx && git -C <wt> commit -qm x`
  - `git -C <S>/fx -C <wt> commit -qm x`
  - `cd <S>/fx && GIT_DIR=<repo>/.git git commit -qm x`
  - `cd <S>/fx && git --work-tree=<wt> commit -qm x`
  - a gitfile repo: `<S>/gf/.git` holding `gitdir: <repo>/.git/worktrees/<id>`; `cd <S>/gf && git commit -qm x`
  - `(cd <S>/fx && :); git commit -qm x`, `true;$(cd <S>/fx && :);git commit -qm x`, `bash -c "cd <S>/fx"; git commit -qm x`, `cd <S>/missing && :; git commit -qm x`
  - `cd <S>/fx && c=commit; git $c -qm x`
  - `cd <S>/fx && loom stage commit <id> -m x`
  - mixed scratch git plus a loom control command: `git -C <S>/fx status; loom stage commit <id> -m x` (every git segment is in scratch, yet `loom stage commit` still blocks)
  - as a subagent: `cd <S>/fx && loom stage complete <id>`
  - `cd <S>/fx && git add -A` (git-add-guard)

## Done when

Your one check is the case table, run once: `cargo test --manifest-path loom/Cargo.toml --test integration hooks_subagent_verify_guard` (a scoped `cargo test`, which the installed guard allows; it compiles what it needs). The orchestrator runs `commit-filter-scratch-repo.sh` at the gate, so list it in your report.
