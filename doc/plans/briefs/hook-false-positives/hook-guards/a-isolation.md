# Worker A: Path Traversal and Git Targets (W2, W4)

Tier: `loom-senior-software-engineer`, effort medium. Wave 2, after worker F. Read `common.md` first; F's functions `loom_track_cwd`, `loom_normalize_path` and `loom_path_class` are in `loom-hooks/_containment.sh`.

## Files you own

- `loom-hooks/worktree-isolation.sh`
- `loom-hooks/_worktree_policy.sh`
- `loom-hooks/tests/worktree-isolation-traversal-resolve.sh` (new)
- `loom-hooks/tests/worktree-isolation-git-target.sh` (new)
- `loom-hooks/tests/worktree-isolation-quoted-payload.sh` (the cases listed under Existing tests)
- `loom-hooks/tests/worktree-shared-policy.sh` (the case listed under Existing tests)

## Read first

- `loom-hooks/worktree-isolation.sh` whole (358 lines). The token path is `validate_bash_command_token_path` (269-286); the regex fallback (294-322) stays as is and still decides every command `loom_tokenize_command` rejects (unterminated quotes, an apostrophe in a `#` comment, `sh -c` nested deeper than 2). The evidence commands all tokenize, so the fallback does not keep their false positives.
- `loom-hooks/_worktree_policy.sh` whole (115 lines): `loom_policy_git_tokens`, `loom_policy_git_fallback` and `LOOM_POLICY_PARENT_PAIR`, loaded from `loom-hooks/worktree-policy.json` (`parent_pair` is `\.\.[\\/]\.\.`). The two policy functions are called only from `worktree-isolation.sh:191` and `:302`; no Rust code parses the JSON (it is only embedded and installed: `constants.rs:16,202`), so W4 needs no Rust change.

## W2: path traversal by resolution

Replace `check_path_traversal_tokens` (211-214) and `check_path_traversal_prefix_tokens` (223-233) with one resolution check:

1. After `loom_tokenize_command` succeeds, call `loom_track_cwd "$(jq -r '.cwd // empty' <<<"$INPUT_JSON")"`.
2. A token is a PAIR candidate when it would match today: a word token containing `$LOOM_POLICY_PARENT_PAIR`, or any token starting with it. A word token is a WIDENED candidate when it is not a pair candidate but contains a single `..` path component (`../x`, `a/../b`, `..`).
3. For each candidate, take the path part. For `--opt=../x` that is the text after the first `=`. Normalize it against `LOOM_TOKEN_CWD[i]`.
4. Outcomes for a PAIR candidate:
   - The cwd is `""`: a warning, not a block. Print `{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"LOOM_HOOK_WARN: ..."}}` with `jq -nc --arg` to stdout and exit 0. The warning names the token and asks for an absolute path.
   - `loom_normalize_path` fails: block.
   - `loom_path_class` gives `worktree` or `scratch`: allow.
   - `sibling`, `state` or `outside`: block with `print_path_traversal_block`. Reword its "You tried to" line to name the resolved path.
5. Outcomes for a WIDENED candidate: an unknown cwd or a failed normalization leaves today's allow, with no warning (`cat "$X/../y"` stays allowed); only a resolution to `sibling`, `state` or `outside` blocks.
6. One command can hold several candidates. A block wins over a warning. Collect warnings into one message.

The cross-worktree check (`check_cross_worktree_tokens`) is unchanged.

## W4: git directory targets

Change `loom_policy_git_tokens` in `_worktree_policy.sh` so that:

- The subcommand word is the first word after the wrapper-unwrapped `git` (`loom_tokens_command_word_index`) that does not start with `-` and is not the value of `-C`, `-c`, `--git-dir`, `--work-tree`, `--namespace`, `--config-env` or `--attr-source`. `git -c a.b=c -C /x status` and `git --no-pager -C /x log` therefore expose their `-C`.
- `-C <dir>`, `--git-dir <dir>`, `--git-dir=<dir>`, `--work-tree <dir>` and `--work-tree=<dir>` count as overrides only as GLOBAL options, between `git` and the subcommand word. `git rev-parse --git-dir`, `git log -C`, `git grep -C 3 x` and `git log --work-tree` are not overrides (today they block, since the token check matches `-C` anywhere in the segment).
- git applies `-C` options one after another: resolve each relative `-C` against the previous one, and `--git-dir`/`--work-tree` against the final `-C`. git rejects `-Cdir` and `-C=dir`, so they need no allowance.
- A global override is ALLOWED only when every resolved target (each `-C` step and the final `--git-dir`/`--work-tree`) has class `worktree` or `scratch`. An unknown cwd, a failed normalization, or any other class blocks: fail closed here, unlike W2, because a retargeted git writes refs. Classification is physical, so `git -C <wt>/.loom/work log` (the symlink into the main state) and `git -C ../.. -C ../.. status` from `<wt>/a/b` block.
- `$LOOM_SCRATCH_DIR` in a `-C` argument resolves through F's expansion rule; any other `$` blocks.
- `GIT_DIR=` / `GIT_WORK_TREE=` assignments and eval-reached git stay blocked exactly as today.
- Reword the `print_git_override_block` "Instead" lines. For `eval`: run the command directly, or loop in plain shell without eval. For `-C`: run git from inside the worktree, or point `-C` at a scratch repo under `$LOOM_SCRATCH_DIR` (written as `"$LOOM_SCRATCH_DIR/<repo>"` or as the literal path).

Make the new allowance depend on `LOOM_TOKEN_CWD` being set for that index, and treat unset as unknown, so a caller that runs before `loom_track_cwd` keeps today's decision.

## Ledger

Call `loom_record_block worktree-isolation <rule> Bash "$COMMAND"` before the `exit 2` at `:347` and before the policy-load exits at `:96-97` (rule `policy-load`). The one `exit 2` at `:347` serves every rule, so each `print_*` call site sets a rule-id global first. Put `[worktree-isolation/<rule>]` on the first non-empty line of each block message (each `print_*` heredoc starts with a blank line).

## Existing tests that change

Run the hook in every test per the `common.md` test-environment rule.

- `worktree-isolation-quoted-payload.sh:84`, `cat ../file.txt` (an allow at HEAD): from the worktree root it now resolves to `<repo>/.worktrees/file.txt` and blocks. Run that case from `$WORKTREE/src` (it then resolves inside and stays an allow), and add the root-cwd form as a new block case.
- `worktree-isolation-quoted-payload.sh:105`, `cat "..\..\file.txt"`: on Unix that is one filename inside the worktree, so it becomes an allow. Flip it.
- `worktree-isolation-quoted-payload.sh:110`, `git -C . status`: becomes an allow. Flip it.
- `worktree-isolation-quoted-payload.sh:97`, `ls ../../../`: stays a block under the test-environment rule; keep it.
- `worktree-shared-policy.sh:52-53` changes `parent_pair` to `fixture-parent` and expects `cat fixture-parent` to block. Under W2 that token resolves to `<wt>/fixture-parent` and is allowed. Replace the case with `cat /x/fixture-parent`: a candidate only through the changed pair (it holds no `..`), resolving to `outside`, so it still proves the policy change reaches the live path. `worktree-shared-policy.sh:39` (`git --work-tree=/tmp status`, expects 2) stays a block, since TMPDIR is never scratch.

## New tests

- `worktree-isolation-traversal-resolve.sh` allows:
  - from `<wt>/web/src`: `sed -n 1p ../../doc/x.md`
  - from `<wt>/web/src`: `git status --short -- . ../components ../../scripts`
  - `cd <wt>/web/src && cat ../../doc/x.md`
  - `cat "$X/../y"` (widened candidate with an unknown base)
- It blocks:
  - from `<wt>`: `cat ../../secret`, `cd web && cat ../../../other/x`, `cat ../<sibling-stage>/x`, `cp x ../../../../etc/y`
  - `cat ../../x` from a directory whose resolution leaves the worktree
  - `cat .loom/work/../../x` from `<wt>` (physical resolution through the state symlink)
- It warns (exit 0, `LOOM_HOOK_WARN` on stdout; a silent exit 0 fails the case):
  - `cd "$X" && cat ../../x`
  - `(cd <wt>/a/b && :) ; cat ../../../x`, `true;$(cd <wt>/a/b && :);cat ../../../x`, `bash -c "cd <wt>/a/b"; cat ../../../x` and `cd <wt>/missing && :; cat ../../../x`: the cwd is unknown, so none of them may resolve against `<wt>/a/b` into the worktree
- `worktree-isolation-git-target.sh` allows `git -C . status`, `git -C <wt>/loom log`, `git -C <S>/clone status` with the scratch path written literally, `git -C "$LOOM_SCRATCH_DIR/clone" status`, `git rev-parse --git-dir`, `git log -C` and `git grep -C 3 x`.
- It blocks:
  - `git -C /abs/repo status` (the main checkout)
  - `git -C ../<sibling> log`
  - `git --git-dir=/abs/repo/.git log`
  - `git -C <wt>/.loom/work log`
  - `git -C .loom/work/../.. status`
  - from `<wt>/a/b`: `git -C ../.. -C ../.. rev-parse --show-toplevel`
  - `git -c x=y -C /abs/repo status` and `git --no-pager -C /abs/repo log`
  - `git -C <S>/link status`, where `<S>/link` is a symlink to `/abs/repo`
  - `LOOM_SCRATCH_DIR=/abs/repo git -C "$LOOM_SCRATCH_DIR" status`
  - `GIT_DIR=x git log`
  - `eval "git log"`
  - `cd "$X" && git -C . log`

## Done when

Both new scripts pass under bash and `LOOM_HOOK_TEST_BSD=1`, run once each. List every existing test you changed and why.
