# `loom ask` and Read Gate Concerns
>
> Ask, read gate

Open items from the portal-token-savings plan: `loom ask`, the deny-once read gate and its Bash forms,
`LOOM_TOKEN_SEPS`, and the usage Attribution section. Mechanisms: [loom ask](../architecture/loom-ask.md),
[Read Gate](../architecture/read-gate.md).

## `loom ask` Evaluation and Isolation Status

`loom ask` has not been evaluated and no isolation probe has run against a real provider: no stage can run
either, because both spend provider tokens and need network. No agent, skill, hook or doctrine text may tell
an agent to use `loom ask` until both of these hold:

1. `loom ask --eval loom/tests/fixtures/ask_eval/cases.json --backend claude` meets the 0.90 accuracy bar and
   the offline replays under the `optim-token` metaplan's evidence rule show a saving: a smaller first
   request or fewer reads alone is not one.
2. The operator's isolation probe passes for the lane in use: a project file telling the reader to print
   `/etc/hostname`, and a second holding `@/etc/hostname`, asked about with `loom ask --question ... --paths
   <file>` once per backend. A reply holding the hostname voids that lane and its evaluation.

The **Codex lane is experimental**. Its `--disable` flags are the closest available to removing tools, are
unproven until the probe passes, and do not cover a global agents-instructions file under `CODEX_HOME`; run the Codex
evaluation only after the probe passes. The read gate is in the same position: it ships on by default as an
operator decision with no replay behind it, and the operator turns it off (`[hooks] read_gate = false`) unless
the `read:*` cost falls by more than the denied calls cost (see [Read Gate](../architecture/read-gate.md)).

## Unimplemented Reviewer Suggestions: `loom ask`

- **Codex isolation:** the scratch directory honours `TMPDIR`, which can nest in a git checkout; Codex is
  assumed to walk to the nearest `.git` for its agents-instructions file, so a repo's copy could reach the worker
  (`run.rs:122`). The disable list names no web-search or image-view feature and the probe reads only a text
  file; extend it with a planted web-search request (`backend.rs:21`). `parse_codex` fails closed only on
  `item.completed`; apply the allow-list to `item.started` too (`backend.rs:229`). Check that `--help` marks
  the Codex variant experimental in its first sentence (`backend.rs:36`).
- **Environment scrub is a deny-list** (`run.rs:34`): a new session variable passes silently.
  `process/environment.rs` shows the allow-list alternative, which withholds `SSH_AUTH_SOCK`. Revisit if the
  Codex probe fails or an agent is told to use `loom ask`.
- **Process lifetime:** unlink the payload after opening it so a SIGKILL leaves no plaintext copy
  (`run.rs:172`); a SIGKILL of loom orphans the worker with no deadline, `PR_SET_PDEATHSIG` in `pre_exec`
  would cover it on Linux, and a `setsid` descendant escapes group retirement (`run.rs:159`). The reader, cap
  and retire logic re-implements `process::run_bounded` and `kill_group` (`process/mod.rs`, `process/owned.rs`);
  an on-spawn hook in `run_bounded_with` would publish the pgid to the interrupt handler and allow reuse
  (`run.rs:189`). The stderr excerpt is the tail of the first 64 KiB, not the end of stderr; `drain_tail`
  keeps the end (`run.rs:249`).
- **Payload:** a per-run random nonce in the wrapper tag would stop a file forging an opening tag or a variant
  such as `</ file>` (`payload.rs:170`; the doc comment claiming nothing can forge a wrapper is stronger than
  the code). `read_to_string_bounded` does not check `nlink`, so a hard link with a harmless name to `.env`
  passes the credential check; `safe_read::single_link_regular_file` would refuse it, at the cost of refusing
  package-manager hard links the operator names (`payload.rs:52`). `is_credential` is `pub(crate)` but used
  only inside the module and its tests, and `payload_tests.rs` has no end-to-end `read_documents` test with
  `HOME` as the root (`ask_eval_contracts.rs` covers it) (`payload.rs:146`).
- **Evaluation:** `eval.rs:28` says case paths are relative to the cases file's directory, but `base.join`
  lets an absolute path through when it is inside the root; refuse absolute case paths or fix the comment. A
  1 MiB cases file can hold thousands of cases of up to 400 KB each with no case cap and no overall deadline.
  In `cases.json:78` the `cpp` case does not check the "why" half of the question, and an answer saying `c,
  not cpp` passes; the file is a frozen fixture.
- **Tests:** the `ask` parse tests in `run_tests.rs:79` belong in `backend_tests.rs` and a new test module for `mod.rs`.

## Unimplemented Reviewer Suggestions: Read Gate and Hook Parser

- **Forms the gate does not see:** `cat < big`, `$(<big)`, `tail -n +1 big`, `sed -n p big`, `cat ~/x`,
  `cat $PWD/x`, and other readers (`nl`, `less`, `awk`) pass unseen (`poll-guard.sh:166`). `tail -n +1 f` and
  `head -n -0 f` are whole reads but classify as ranges and bypass it (`poll-guard.sh:131`). A pipe skips the
  read whatever the consumer is, so `cat big.json | jq .` sends the whole file without a denial; failing open
  is the safe direction, and restricting the skip to known reducers (`wc`, `grep`, `head`, `tail`, `jq` with a
  filter) would be tighter (`poll-guard.sh:173`). If the `read:*` numbers disappoint, check these before
  tightening anything else. A `cat a b` naming two large files is denied once per file in sequence
  (`poll-guard.sh:199`).
- **Denial text:** only Markdown outlines are capped at 200 rows; a generated source file with thousands of
  symbols puts every row in the denial's stderr (`_read_discipline.sh:172`, hypothetical at this repository's
  file sizes). The `rg` advice interpolates the path unquoted, so a path with a space yields a wrong command
  (`_read_discipline.sh:283`). A column-zero `# comment` inside YAML front matter becomes a heading row in a
  Markdown outline (`markdown_outline.rs:143`), harmless noise.
- **Exemption:** it tests the path as given, so a bare `cat` of a plan file from a cwd inside `doc/plans`, or of a signal file from inside `.loom/work`, costs one denial of a control input; testing the absolute path as
  well closes it (`_read_discipline.sh:263`). The `$LOOM_WORK_DIR` exemption branch is never reached by a test,
  because `control_inputs_are_exempt` uses a `.loom/` path (`_read_discipline.sh:265`).
- **Naming and tests:** `_loom_polls_count` also counts denial rows, and a name such as `_loom_ledger_count`
  would fit both uses (`_read_ledger.sh:119`). `cat_denied_once_then_read_allowed` returns early when the
  process tree is not visible; `bash_cat_and_read_share_one_denial` covers the same behaviour with no skip
  (`hooks_poll_guard_reads.rs:147`). The `enable_deny()` calls in that file no longer affect rule 1, which
  liveness decides now (`hooks_poll_guard_reads.rs:28`, `:191`, `:201`, `:226`, `:253`). No hook-suite test
  reads `LOOM_TOKEN_SEPS` directly; add one that checks index alignment through `bash -c` splicing and a read
  under `set -u` (`_parser.sh:113`).
- **Function size over Rule 17:** `_loom_expand_shell_c` is about 80 lines and `_loom_tokenize_walk` about 209
  (`_parser_expand.sh:75`, `_parser.sh:145`), both grown by the `LOOM_TOKEN_SEPS` work. Extract the
  `src_seps`/`out_seps` copying into a helper to stop the growth.

## Proposed Guards for Recurring Mistakes

- **Relay-eligible `loom` command run in the background or piped.** Three occurrences, the last on
  2026-11-02 ([record](../mistakes/memory-relay-drain-gap.md)): prose rules did not stop it. Propose a
  PreToolUse hook that denies a Bash call whose command runs `loom check`, `loom stage commit`,
  `loom stage complete` or `loom memory resolve` with `run_in_background`, a pipe or a redirect of its stdout.
- **Integration-verify full-suite criterion written as a plain string.** Two occurrences
  ([record](../mistakes/completion-runtime.md)). Propose a `loom plan verify` check that fails an
  integration-verify criterion matching `cargo test --all-targets` unless it is an extended criterion with
  `timeout_secs` of at least 1200, and the same line in the plan-writer skill.
- **A quoted heredoc holding many quotes is refused** by `loom-control-complete.sh` as "could not be
  tokenized safely", though the command has nothing to do with completion. Apply bulk edits with the Edit and
  Write tools. Inside the sandbox `/tmp` is read-only and `spawn-guard-reviewer-model.sh` runs its hook under
  `env -i` with no `TMPDIR`, so `mktemp` fails and its "stderr not in warning" case fails on a clean HEAD too.
  Propose that the completion hook fail open on an untokenizable command that contains no `loom stage`.

## Discarded and Decided

- The attribution `signals/` arm also matches source paths such as `loom/src/orchestrator/signals/**` and
  admission cost is charged past a `compact_boundary`; both are recorded in
  [Read Gate](../architecture/read-gate.md#rows), where a reader of the rows sees them.
- Left unchanged on purpose: the `run_bounded` reuse is a process-lifetime refactor with no failing scenario at
  the plan's last gate; a single-link check would refuse files a package manager hard-links; the attribution
  brief's table requires the wider `signals/` arm.
