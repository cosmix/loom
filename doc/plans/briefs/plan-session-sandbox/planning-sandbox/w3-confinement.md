# W3: the real-sandbox confinement test and the user docs

Read `doc/plans/briefs/plan-session-sandbox/common.md` first. You run in wave 2, beside W2, after
W1 has returned (you call `plan_session::sandbox::build_settings` and `write_session_sandbox`).

## Files you own (write only these)

- `loom/tests/plan_session_confinement.rs` (a new integration-test target)
- `loom/tests/plan_session_confinement/srt.rs`, `loom/tests/plan_session_confinement/fixture.rs`
(The README entry and the loom-usage sentence below are codex units C10 and C11; the Docs
section stays as their specification.)

## The confinement test

Section 12 acceptance: inside a real planning sandbox, `loom knowledge update`,
`loom plan summary seal`, `loom map` and `loom knowledge context` work, and a raw write to code and
to `.git` is refused.

Precedent to mirror (read it, do not import it: it is an in-crate `cfg(test)` module, invisible to
an integration test, and the probes here need the built binary, which only an integration test
gets as `env!("CARGO_BIN_EXE_loom")`):
`loom/src/orchestrator/terminal/native/tests_confinement_e2e.rs` (fixture outside `/tmp`, lines
45-75; `#[serial]`) and `tests_confinement_srt.rs` (the `skip` rule, lines 40-93; the ALIVE probe
protocol, lines 1-20 and 251-354; the settings translation `srt_settings`/`srt_document`/`srt_path`,
lines 130-250). Copy only what you need into `plan_session_confinement/srt.rs`, keeping:

- the skip: Linux only, and skipped unless `bwrap`, `socat` and `srt` are on PATH and
  `srt -c true` succeeds, ALWAYS through `loom::process::sandbox_probe::skip_unless` (or the same
  rule: `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns a skip into a panic), so the operator run that
  integration-verify blocks on cannot pass by skipping. The stage's `before_stage` requires `srt`
  on the host (`@anthropic-ai/sandbox-runtime` 0.0.78, `architecture/execution-containment.md`
  "Confinement E2E Lives Outside the Sandbox"), so you can read its README and settings schema
  beside `readlink -f "$(command -v srt)"` before writing the translation. In the criteria runner
  and the session the test still skips (nested sandbox, no Unix sockets); record that with
  `loom memory note`.
- the ALIVE protocol: a probe counts only when its shell printed `ALIVE` first.
- the translation, extended with `allowRead`: pass the settings' `sandbox.filesystem.allowRead`
  entries to srt as `filesystem.allowRead`, after confirming from srt's own schema that it accepts
  the key. There is NO fallback: production sessions depend on that carve-out under the runtime
  deny, so when srt cannot express it the `seal` probe must fail and its message must name the
  missing carve-out (a passing test with the runtime moved out of the deny would hide exactly the
  layout bug it exists to catch).

Fixture (`fixture.rs`): a temp base outside `/tmp` (as `tempdir_outside_tmp`, e2e lines 60-72),
holding a fake home, a git repository (`git init`, one commit) with `loom/src/lib.rs`, `README.md`,
`doc/product/index.md`, `doc/plans/PLAN-a.md` (a minimal valid plan: copy the smallest plan YAML
from `loom/tests/fixtures/` that `loom plan verify` accepts), `doc/loom/knowledge/` initialised by
running `loom knowledge sync` outside the sandbox first if the update needs it, and a summary
draft ready to seal (use plan 4's `loom plan summary brief` + a hand-written draft under the
fixture's `drafts/`, following the format plan 4 documents in
`doc/plans/DONE-PLAN-plan-summary-store.md` or the summary format topic in `doc/loom/knowledge/`;
if sealing needs a summary of `doc/product/index.md`, write exactly one sentence citing it).
Build the authoring settings with `write_session_sandbox` against that root, a runtime from
`planning::runtime::runtime_paths_under(<fake loom home>, root)`, and `loom_bin` =
`env!("CARGO_BIN_EXE_loom")`.

Spawn the loom binary only through a local `fn loom_cmd()` that removes `LOOM_STAGE_ID`,
`LOOM_SESSION_ID`, `LOOM_WORK_DIR` and every other `LOOM_*` variable and sets `LOOM_HOME` to the
fixture's loom home (the rule `tests/integration/binary_spawn_guard.rs` enforces for raw spawns:
bind the path to a `const LOOM: &str = env!(..)` as `tests/web_host.rs:17` does, never
`Command::new(env!(..))`). Inside srt the same scrub is done by `env -u`.

Probes, each under the authoring settings, cwd = the fixture root, `HOME` = the fake home:

1. `loom knowledge update conventions "<entry>"` exits 0 and the knowledge file gained the entry.
2. `loom plan summary seal <id>`, run with `LOOM_PLAN_SESSION=<a session id>` and
   `LOOM_PLAN_STATE=<runtime dir>` set exactly as plan 7's launch sets them, exits 0 and the
   summary file exists under `doc/loom/summaries/` (this proves plan 1's in-session check, which
   accepts the variable only when the planning root is not writable, passes inside the sandbox);
   and `touch <planning root>/probe` is refused (the planning root is write-denied for every kind:
   only `drafts/`, `claims/` and `summaries.lock` are granted).
3. `loom map --outline loom/src/lib.rs` exits 0 and prints the file's outline, with `.loom`
   write-denied (the graph store falls back to memory: `architecture/source-graph.md`, "a
   write-denied cache falls back to memory").
4. `loom knowledge context --query "conventions" --budget-tokens 500` exits 0.
5. `echo x > loom/src/lib.rs` is refused (file unchanged) and `echo x > .git/probe` is refused
   (no file).
6. `echo x > doc/product/new.md` succeeds (the doc tree stays writable to Bash), and
   `echo x > doc/plans/IN_PROGRESS-PLAN-b.md` is refused when that file existed at build time.

Deny probes (px-security-03), each preceded by the ALIVE check and each required to FAIL:

7. `cat <runtime>/lock` (a lock file the fixture writes holding a fake token) and
   `cat <runtime>/sessions/s/answers/a.json`.
8. `cat <fake home>/.ssh/id_rsa` (a file the fixture writes).
9. `cat .loom/work/admin.token` (the fixture creates `.loom/work/` with that file).
10. a TCP connect to a loopback listener the test opens outside the sandbox
    (`bash -c 'exec 3<>/dev/tcp/127.0.0.1/<port>'`), and a Unix-socket connect to a socket the
    test binds outside the sandbox (`socat - UNIX-CONNECT:<path>`): the tmux and server-port
    self-approval routes.

Each probe is its own `#[test]` with `#[serial]` (`serial_test` is a dev-dependency) so a failure
names the probe. Each file stays under 400 lines, each fn under 50.

## Docs

- `README.md` under `### Plan Commands`: `loom plan hook <event> --session-dir <dir>` as a hidden
  internal entry that Claude Code runs for planning sessions (the PreToolUse path guard), one or
  two sentences, no usage example (users never run it). Match the surrounding entry format.
- `skills/loom-usage/SKILL.md`: one sentence that `loom plan hook` is internal and never run by
  hand. No completions entry (the command is hidden; metaplan "Doctrine sync").

## Proof

`cargo test --test plan_session_confinement` from `loom/` (passes, or every test prints its skip
reason and passes). Report whether srt ran, the allowRead decision, and anything surprising.
