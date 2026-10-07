# U1: the `stage commit` subcommand module

Codex unit (`loom-codex-forwarder`, gpt-5.6-terra, effort xhigh), one module plus its test file, three
numbered steps.
Read `../common.md` first (its section "Pinned interfaces: daemon-owned-commits", bullet for the commit
subcommand), then this brief.

## Role and issue

Issue #22: a stage session cannot sign commits inside its sandbox, so it asks the daemon to commit. You
write the session side of that request: the module behind the `stage commit` subcommand of the loom CLI.
The clap variant, its dispatch and the `pub mod commit;` line are written by a different worker (C2); the
git core and the daemon handler are written by C1 and C2. You write the module and its test file.

HARD RULES for you, the codex agent:

- Never run any git command that writes (no add, no commit, no checkout, no reset, no stash, no config
  write). Never run git at all except where a test body you write does so on a throwaway fixture repository.
- Never read, write or list anything under a `.loom/` path.
- Do not run cargo. The orchestrator runs the proof command.
- Write no placeholders, no stubs, no unfinished markers.

## Files owned and files to read

Own exactly two files: `loom/src/commands/stage/commit.rs` (new) and `loom/src/commands/stage/commit/tests.rs`
(new; declared at the end of `commit.rs` as `#[cfg(test)] mod tests;`, so the module path is
`commands::stage::commit::tests`). Keep each file at or under 400 lines and every function under 50 lines.

Read, in this order:

- `loom/src/commands/stage/merge/relay.rs` lines 1-70: the pattern to copy (`mode`, `EnvSnapshot`,
  `RelayContext::check`, `RelayContext::emit`) and, in its tests, `VecSink` and `context_for`.
- `loom/src/relay/emit.rs` lines 85-250: `RelayMode`, `RelayContext`, `check`, `emit`.
- `loom/src/relay/emit/test_support.rs`: `context_for` and `RelayFixture` (cfg test only).
- `loom/src/process/mod.rs` lines 285-300: `run_bounded_output`.
- `loom/src/git/runner.rs` lines 134-200: `run_git_checked` (it forces hooks off, so it is for
  `rev-parse` and `write-tree` only, never for running a hook).
- `loom/src/relay/payload.rs` lines 25-45: the payload types sit here; `CommitPayload` is added by C2.
- `loom/src/git/stage_commit.rs`: C1 writes it in parallel; `validate_commit_message` lives there.

## Pinned interfaces it provides and consumes

Provides: `pub fn execute(stage_id: String, message: String) -> anyhow::Result<()>`, called by C2 as
`commit::execute(stage_id, message)`. It refuses a `stage_id` other than the environment variable
the session's stage id when one is set (read from `EnvSnapshot::from_process_env().stage_id`, never
`std::env::var`); validates the message through `crate::git::stage_commit::validate_commit_message` (C1's
function: non-empty, at most 16 KiB, no NUL byte, no AI attribution; you keep no copy of its rules); runs the `pre-commit` and `commit-msg` hooks of the repository inside the sandbox through
`git hook run`; normalises the message with `git stripspace`; reads `git rev-parse HEAD` and
`git write-tree` AFTER the hooks; then relays a request of
kind `RequestKind::Commit` carrying a `CommitPayload` through `RelayContext::check` and
`RelayContext::emit`, exactly as the merge relay module does. In operator and legacy mode it calls
`crate::git::stage_commit::commit_staged` in-process instead. No `--amend`, `--author`, `--no-verify` or
`--no-gpg-sign` exists.

Consumes (written by other workers in parallel; the crate does not compile until all return):

- `crate::relay::CommitPayload { pub message: String, pub expected_head: String, pub expected_tree: String }`
  (serde, deny unknown fields).
- `crate::relay::RequestKind::Commit`.
- `crate::git::stage_commit::validate_commit_message(message: &str) -> Result<(), String>` (the `Err` text
  names the refusal).
- `crate::git::stage_commit::{commit_staged, CommitRequest, CommitScope}`: `commit_staged(repo: &Path, scope:
  &CommitScope, request: &CommitRequest) -> Result<String, CommitRefusal>` returns the new commit id;
  `CommitRefusal` implements `Display`; `CommitScope::StageBranch { stage_id: String }`; `CommitRequest {
  message, expected_head, expected_tree }` (all `String`).

## Root cause and current behaviour

There is no such module today: every session runs the commit itself inside its sandbox, where the signing
agent directory is unreadable. `commands/stage/mod.rs` has no `commit` entry at `ff3fe947`. The relay
machinery you call already exists and is wired for other kinds (`merge/relay.rs`). The git runner in this
crate passes `-c core.hooksPath=/dev/null` on every call, so `git hook run` must be started with a plain
`std::process::Command`, never through `run_git_checked`, or no hook would run.

## Step-by-step tasks

1. Validation, hooks and state readers (private functions, each under 50 lines).
   - Message validation is `crate::git::stage_commit::validate_commit_message`, mapped with
     `.map_err(anyhow::Error::msg)`. Write no local validator and no local attribution check: C1 owns the
     rules (refuse empty after trim, more than 16 * 1024 bytes, any NUL byte, and AI attribution: a
     `co-authored-by:` or `signed-off-by:` line that names claude or anthropic, any `noreply@anthropic`,
     `generated with` together with `claude code`, `claude.ai` or `claude.com`; plain prose that merely says
     claude passes), and the daemon applies the same function again.
   - `fn run_hooks(cwd: &Path, message: &str) -> Result<String>`: run `git hook run --ignore-missing
     pre-commit` in `cwd`; then write the message to a `tempfile::NamedTempFile`, run `git hook run
     --ignore-missing commit-msg -- <that path>`, read the file back (a hook may rewrite it), then
     normalise it: run `git stripspace` with the message file as the command's stdin
     (`Stdio::from(File::open(path)?)`; this is `git commit -m`'s default whitespace cleanup: trailing
     whitespace and surplus blank lines dropped, one final newline) and take its stdout. Validate that
     result again through `validate_commit_message` (a rewrite or the cleanup can empty it) and return it;
     it is the message the payload carries. All spawns use `std::process::Command::new("git")` with
     `current_dir(cwd)` and `crate::process::run_bounded_output(&mut command, Duration::from_secs(600),
     "<label>")`. A non-zero exit bails with the hook name (or `stripspace`) and the last 20 lines of its
     stderr and stdout.
   - `fn staged_state(cwd: &Path) -> Result<(String, String)>`: `run_git_checked(&["rev-parse", "HEAD"],
     cwd)` and `run_git_checked(&["write-tree"], cwd)`, in that order, after the hooks.
2. The command.
   - `pub fn execute(stage_id: String, message: String) -> Result<()>`: `let env =
     EnvSnapshot::from_process_env();` then `mode(&env)`, `std::env::current_dir()`, `env.stage_id.as_deref()`
     as the session stage, `StdSink::default()`, then `commit_with(...)`. The stage id comes from the
     snapshot, never `std::env::var`: the snapshot is empty under `cfg(test)`, so no test depends on the
     process environment.
   - `fn commit_with(stage_id: &str, message: &str, session_stage: Option<&str>, relay_mode: RelayMode, cwd:
     &Path, sink: &mut dyn RelaySink) -> Result<()>`, in this order: validate the message; refuse a
     `session_stage` that differs from `stage_id`; for `RelayMode::Relay(context)` call
     `context.check(RequestKind::Commit, Some(stage_id), cwd, uid)` first (uid from `libc::getuid()` in an
     `unsafe` block with a SAFETY comment, as `merge/relay.rs` does) so a refusal costs no hook run; run the
     hooks; read the state; build `CommitPayload`; `context.emit(RequestKind::Commit,
     serde_json::to_value(&payload)?, "stage commit", false, sink)?`; then write to `sink.stderr()` one line:
     `Wait for it in your NEXT Bash call: loom request status <id> --wait 90` with the id from the returned
     `RelayLine`. Nothing else goes to stdout: the relay line must stay the last stdout line.
   - For `RelayMode::Operator` and `RelayMode::Legacy`: hooks, state, then the repository top level from
     `run_git_checked(&["rev-parse", "--show-toplevel"], cwd)`, then `commit_staged(Path::new(&top),
     &CommitScope::StageBranch { stage_id: stage_id.to_string() }, &CommitRequest { message, expected_head,
     expected_tree })`; map the refusal with `anyhow::anyhow!("{refusal}")`; print `committed <id>` through
     `sink.stdout()`.
3. Tests, in `commit/tests.rs` (see the next section for names). Build every fixture repository in a
   `tempfile::TempDir`.

## Tests to write

Module `commands::stage::commit::tests` (file `commit/tests.rs`, `use super::*;`). Every fixture git call sets the three variables
`GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` (pointing at files that do not exist) and
`GIT_CONFIG_NOSYSTEM=1`, and the repository config sets `user.name`, `user.email`, `commit.gpgsign=false` and
`core.hooksPath=.git/hooks` locally (pattern: `loom/src/verify/impact_tests_tests.rs`). Never mutate the
process environment; `commit_with` takes its inputs as arguments. A `VecSink` implementing `RelaySink` with
two `Vec<u8>` buffers is the sink (copy the one in `merge/relay.rs` tests).

- `an_empty_message_is_refused`, `a_nul_byte_is_refused`, `an_oversized_message_is_refused` (each through
  `commit_with`, so they prove the call into `validate_commit_message`, not a second validator).
- `ai_attribution_is_refused`: a `Co-Authored-By: Claude <noreply@anthropic.com>` trailer, a `Signed-off-by:
  Claude` line and a `Generated with Claude Code` line each refuse; `a_plain_message_naming_claude_passes`
  (`feat(auth): add the claude login probe`).
- `a_stage_id_other_than_the_session_stage_is_refused` (call `commit_with` with `session_stage` set to another
  id; no ticket is written).
- `the_payload_carries_the_tree_written_after_a_restaging_pre_commit_hook`: use `context_for(SessionType::Stage)`
  from `relay::emit::test_support`; `git init` inside `fixture.cwd`, commit a base file, stage a change to
  `a.txt`, record `write-tree` as `before`; install an executable `.git/hooks/pre-commit` script that appends
  one line to `a.txt` and runs `git add a.txt`; call `commit_with` with `RelayMode::Relay(fixture.context.clone())`
  and the context stage id (`stage-a`); read the single `*.req` file in `fixture.context.scratch_dir`, decode it
  with `crate::relay::Ticket::decode`, decode the payload into `CommitPayload`, and assert `expected_tree` equals
  the repository `write-tree` now and differs from `before`, and `expected_head` equals `rev-parse HEAD`.
- `a_failing_pre_commit_hook_writes_no_ticket`.
- `a_commit_msg_hook_rewrite_reaches_the_payload`.
- `operator_mode_calls_commit_staged_in_process`: a repository whose checked-out branch is `loom/stage-a`,
  one staged file, `RelayMode::Operator`; afterwards `git log -1 --format=%s` shows the message and the printed
  stdout holds `committed` followed by the head id.
- `the_confirmation_names_request_status_wait` (stderr contains `loom request status` and `--wait 90`).
- `the_message_is_stripspaced_after_the_hooks` (a `commit-msg` hook that appends trailing spaces and two
  blank lines; the payload message has neither and ends in one newline).

## Patterns to copy, and the property not to copy

Copy `commands/stage/merge/relay.rs`: the `mode` call, the testable inner function that takes `RelayMode`,
`cwd` and a `&mut dyn RelaySink`, the `getuid` block, and the single `emit` call that prints the relay line
last. Do NOT copy any fallback to a socket or a spool: this command has no path other than the relay (and the
in-process call in operator and legacy mode). Do not copy `run_git_checked` for hooks (it disables them).

## Traps

- Knowledge (`architecture/security-and-isolation.md`, Session Requests Travel Through a Hook-Written Inbox):
  the CLI writes a ticket and prints one line ending in the relay marker; the relay hook reads it only after
  the Bash call returns. So the wait for the result must be a separate later call; say so in the stderr line.
- Knowledge (`mistakes/concurrency-and-locking.md`): an inbox ledger holds two rows per request id, so any
  reader takes the latest row. You write no reader; U2 does.
- The hooks may restage files: read `HEAD` and the tree strictly after both hooks ran, never before.
- `git hook run` needs git 2.36 or later; loom already requires 2.40.
- Existing assertion lines in other test files are never edited; you create a new file, so this does not
  apply to you, but your tests must not rely on the real `HOME` or on the live state directory.

## The one check

None for you (do not run cargo). The orchestrator runs `cargo test --lib commands::stage::commit` once after
all six workers of this stage return.

## Report format

Reply with: the files written and their line counts; the public and private function list; the assumptions you
made about `CommitPayload`, `RelayLine` and `commit_staged`; anything in this brief that the code contradicts.
