# Worker F: Shared Resolver and Block Ledger

Tier: `loom-senior-software-engineer`, effort medium. Wave 1: the hook workers A to D start only after you return.

Read `common.md` in this directory first. It pins the four functions you write, the `LOOM_SCRATCH_EXPAND` global and their exact semantics, and the test-environment rule.

## Files you own

- `loom-hooks/_containment.sh`: add `loom_track_cwd`, `loom_normalize_path`, `loom_path_class` and a private `_loom_owned_by_uid`.
- `loom-hooks/_common.sh`: add `loom_record_block`.
- `loom-hooks/tests/containment-cwd-resolve.sh` (new).
- `loom-hooks/tests/common-block-ledger.sh` (new).

## Read first

- `loom-hooks/_containment.sh` whole (119 lines): `loom_cwd_worktree` (74-82, logical `pwd` and a greedy regex that picks the innermost `.worktrees`) and `loom_current_worktree` (109-119). `loom_path_class` derives the worktree root from `loom_current_worktree`, and the repo root as the parent of `.worktrees/`.
- `loom-hooks/_parser.sh:66-139`: the tokenizer contract, `LOOM_TOKENS`, `%%SEP%%`, `LOOM_TOKEN_SEPS` (documented at 113-117). `loom-hooks/_parser_expand.sh:98-151`: how an `sh -c` body is spliced in with an empty SEPS entry.
- `loom-hooks/_parser_scan.sh:30`: `loom_token_is_word`.
- `loom-hooks/_post-tool-heartbeat.sh:128-136`: the heartbeat directory convention the ledger follows.
- `loom-hooks/_identity.sh:255-270`: `loom_payload_agent_verdict` (prints `subagent` for any non-empty `.agent_type`).
- `loom-hooks/worktree-file-guard.sh:151-157`: `owned_by_uid`, which you copy as `_loom_owned_by_uid`; do not source the guard.
- Two existing tests as a pattern: `loom-hooks/tests/common-token-helpers.sh` (unit-style tests of sourced helpers) and `loom-hooks/tests/worktree-isolation-quoted-payload.sh` (a fake `.worktrees/<id>` tree).

Tokenizer facts observed at HEAD, so you need not re-derive them: `"$X"` becomes the word `$X`; `2>&1` becomes `2`, a sentinel with SEPS `>&`, `1`; `$(` adds no character to SEPS; `(cd a && :) ; x` gives `[SEP:(] cd a [SEP:&&] : [SEP:);] x`; an apostrophe inside a `#` comment makes tokenize fail (rc 1).

## Steps

1. `loom_normalize_path`: string work plus `(cd -P -- "$p" && pwd -P)` on existing directory prefixes, exactly as `common.md` pins; no `realpath`, no `readlink -f`. The physical step matters: every worktree has `<wt>/.loom/work -> ../../../.loom/work`, so `<wt>/.loom/work/../x` is `<repo>/.loom/x`, and lexical normalization would call it `<wt>/x`. It also maps macOS `/var` to `/private/var`.
2. `loom_track_cwd`: fill `LOOM_TOKEN_CWD` and set `LOOM_SCRATCH_EXPAND` exactly per the `common.md` rules. Reset both at entry so a second call in one process starts clean. Callers that never call it see `LOOM_SCRATCH_EXPAND` unset, which `loom_normalize_path` treats as 0.
3. `loom_path_class`: the classes and their order are pinned in `common.md`. Use `_loom_owned_by_uid` for the scratch test. LOOM_SCRATCH_DIR under the repo is classified by the repo rules first.
4. `loom_record_block`: per `common.md`. It writes only under `$LOOM_WORK_DIR/heartbeat/`, creating that directory mode 700 when missing, and refuses a heartbeat path that is a symlink. It applies the ledger file safety rules in `common.md` (regular file or absent, no symlink, 4 MiB cap, `umask 077`). It writes nothing and returns 0 when any precondition fails, and it never writes to stdout or stderr.
5. Tests. Follow the `common.md` test-environment rule; create every directory a case names.
   - `containment-cwd-resolve.sh` covers:
     - `cd <wt>/web/src && sed -n 1p ../../doc/x.md`: the second segment's cwd is `<wt>/web/src`, and the path normalizes to `<wt>/doc/x.md`.
     - `cd "$X" && cat a`: unknown.
     - `cd web | cat ../x`: unknown.
     - `(cd web && cat x)`: unknown after the `(`.
     - `(cd <wt>/a/b && :) ; cat ../../x`, `true;$(cd /a && :);cat ../x`, `bash -c "cd <wt>/a/b"; cat ../../x`: the last segment is unknown (taint).
     - `cd <wt>/missing && :; cat ../../x`: unknown after the `;` (unverified cd).
     - `mkdir -p <S>/fx2 && cd <S>/fx2 && git init -q`: the `git` segment's cwd is `<S>/fx2` (an `&&` chain after an unverified cd).
     - `cd web; cat x`: known.
     - `loom_normalize_path / ../..`: prints `/`.
     - `loom_normalize_path <wt> .loom/work/../x`: prints `<repo>/.loom/x`.
     - `~user/x`: returns 1. `$X/y`: returns 1. `$LOOM_SCRATCH_DIR/clone` with `LOOM_SCRATCH_EXPAND=1` and a valid scratch dir: prints the expanded path; with `LOOM_SCRATCH_EXPAND=0`: returns 1.
     - `LOOM_SCRATCH_EXPAND` is 0 for `LOOM_SCRATCH_DIR=/x git -C "$LOOM_SCRATCH_DIR" status` and for `export LOOM_SCRATCH_DIR=/x; git -C "$LOOM_SCRATCH_DIR" status`, and 1 for `git -C "$LOOM_SCRATCH_DIR/clone" status`.
     - `loom_path_class` for each of the five classes, including `<repo>/.worktrees/stage-x` as `sibling` when the current worktree is `<repo>/.worktrees/stage`, `<wt>/.loom/work/x` (physical) as `state`, a TMPDIR path as `outside` (TMPDIR is never scratch), and a LOOM_SCRATCH_DIR that is a symlink or owned by another uid as not scratch (print SKIP when the test cannot create the other-uid case).
   - `common-block-ledger.sh` covers: one appended line with every field; `role=subagent` when the payload carries `agent_type`; subjects of 2-, 3- and 4-byte characters cut at every offset around 300 bytes, each result valid UTF-8 (check with `iconv -f UTF-8 -t UTF-8` or `jq -R`) and at most 300 bytes; nothing written and exit 0 when `LOOM_STAGE_ID` is unset or holds `../x`, when `LOOM_CODEX_FORWARD=1`, and when `$LOOM_WORK_DIR/heartbeat` is a symlink; nothing written and a prompt exit 0 when the ledger path is a FIFO (run under `timeout 5`), a symlink to another file (the target stays byte-identical), or a regular file over 4 MiB; a newly created ledger has mode 600 (`stat -c %a`, BSD `stat -f %Lp`); nothing ever printed to stdout or stderr.

## Done when

Both new test scripts pass under `bash` and under `LOOM_HOOK_TEST_BSD=1 bash`. Run each once.

## Report

Function signatures as written, test names, anything in `common.md` you found ambiguous and how you settled it.
