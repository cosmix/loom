---
aliases:
- loom ask
- ask eval
- one-shot worker
sources:
- loom/src/commands/ask/mod.rs
- loom/src/commands/ask/backend.rs
- loom/src/commands/ask/run.rs
- loom/src/commands/ask/payload.rs
- loom/src/commands/ask/eval.rs
- loom/tests/fixtures/ask_eval/cases.json
verified: 3e22bf84e13e65479097f1a8206ca9bd0158ff61
---

# `loom ask`
>
>

`loom ask` answers one question about named files with a one-shot worker that has no tools, no settings
and no project instructions. `loom ask --eval <cases.json>` scores a backend against a cases file. It exists
to measure whether a cheap worker can answer a question about files for fewer tokens than reading them. It is
an evaluation instrument, **not** a tool any agent may use: no agent, skill, hook or doctrine text names it.
Evaluation status and the open concerns are in [`loom ask` and Read Gate Concerns](../concerns/loom-ask-and-read-gate.md#loom-ask-evaluation-and-isolation-status).

## Command Surface (`loom/src/commands/ask/mod.rs`)

`--question <Q> --paths <P>...` (files inside the project, 400,000 bytes in total) or `--eval <CASES>`; the
two modes are an exclusive clap group. `--paths` carries an explicit `conflicts_with = "eval"`, because clap
skips a `requires` whose target conflicts with a present argument and would otherwise parse `ask --eval c
--paths a`. `--backend claude|codex` (default `claude`), `--model` (default `haiku` on Claude, the luna tier on
Codex), `--timeout-secs` (default 180, 1 to 86400), `--json`. A refusal or worker failure exits 1, a signal
130, `--eval` exits with its own verdict.

## Lanes (`backend.rs`)

- **Claude:** `claude -p` with `--tools` empty, `--setting-sources` empty, `--strict-mcp-config`,
  `--disable-slash-commands`, `--no-session-persistence` and the mode prompt in `--system-prompt`.
- **Codex (experimental):** `codex exec --json --sandbox read-only --skip-git-repo-check --ephemeral
  --ignore-user-config --ignore-rules` with `--disable` for `shell_tool`, `unified_exec`, `apps`,
  `browser_use`, `computer_use`, `multi_agent` and `plugins`. `codex exec` has no switch that removes every
  tool, and `--ignore-user-config` does not cover a global agents-instructions file under `CODEX_HOME`. The contracts drive
  a fake binary, so nothing in the tree proves these flags keep a real worker from using tools. The lane stays
  experimental until the operator's isolation probe passes against the real provider (see the README
  "After merge" text and the plan's probe). `parse_codex` fails closed on a tool item that completes.

## Isolation (`run.rs`, `payload.rs`)

The payload reaches the worker on stdin from a private file in a scratch directory used as the worker's cwd.
The worker runs in its own process group with a deadline; an interrupt handler kills the group. The
environment drops every `LOOM_*` variable and the seven `SESSION_VARS` of an enclosing agent session
(`is_scrubbed`), a deny-list by spec. Stdout is capped at 16 MiB, stderr at 64 KiB. `read_documents` refuses a
credential (`CREDENTIAL_NAMES`, prefixes, suffixes, and the sandbox deny-read paths), a path that leaves the
project root, a symlink, non-UTF-8 content and content past the byte cap. Files are wrapped in `<file path=...>`
tags whose closing form is escaped in any ASCII case, and the path attribute escapes quote and backslash.
File contents are data to the worker (the mode prompt says to ignore instructions inside them).

## Evaluation (`eval.rs`)

A cases file is a JSON array (at most 1 MiB) of `{id, question, paths, must_contain, must_not_contain?}`, with
no unknown fields, no empty id, question, paths or `must_contain`, and no duplicate ids. Paths are joined to
the cases file's directory. Cases run one at a time. The report gives accuracy against `ACCURACY_BAR` (0.90),
worker tokens, latency percentiles over answered cases only, and what reading the files directly would have
cost. A refused path is reported as refused, apart from worker failure, and counts against accuracy. The
fixture is `loom/tests/fixtures/ask_eval/cases.json`.

## Rules for Changing It

- A contract fake that must fill a pipe uses the shell's `printf` builtin: `yes '' | head -c 204800` finishes
  with no reader and leaves 204800 bytes in a 65536-byte pipe, so it never blocks and cannot catch a stdin
  write that deadlocks.
- A wiring check on `mod eval_tests;` can never pass: a test-module declaration is a definition and the check
  wants a consumer. Dispute it with `--field wiring`.
- The `dispatch` ledger row in the maintainability baseline is 57 after the `Project` arm moved to
  `dispatch_project` and the `Ask` arm was added.
