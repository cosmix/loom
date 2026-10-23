# W4: the manual-mode call sites and the release checklist

Tier: sonnet (`loom-software-engineer`). Spawned after the codex unit X1 has returned. Read `doc/plans/token-optim/briefs/sandbox-route-closure/common.md` first.

## Goal

1. `loom run --manual` prints a bare `claude` command at two sites, and a session started from it runs with none of the wrapper's confinement. X1 wrote `src/orchestrator/core/manual_start.rs`, whose two builders return each site's lines with a notice line after the "To start:" line. You declare that module and make both sites print through it.
2. A manual release checklist holds the live, credentialed checks no test in this repository can make, and `CONTRIBUTING.md` and `README.md` point at it.

## Files you own

- `src/orchestrator/core/mod.rs` (one `mod` line)
- `src/orchestrator/core/stage_spawn.rs` (`print_manual_start` body only)
- `src/orchestrator/core/stage_knowledge.rs` (the manual branch of `spawn_knowledge_worker` only)
- `../doc/sandbox-release-checklist.md` (new)
- `../CONTRIBUTING.md` (one paragraph in "### Releases")
- `../README.md` ("## Sandbox Configuration": the YAML example, its "Note:" paragraph, and the first paragraph of "### Session State Confinement")

Read only: `src/orchestrator/core/manual_start.rs`, which X1 wrote. When the codex unit failed or could not run, your prompt also names `doc/plans/token-optim/briefs/sandbox-route-closure/route-closure/x1-manual-start.md`: then you own `src/orchestrator/core/manual_start.rs` too and write it by that brief first. Otherwise, if the file is missing or its signatures differ from the ones below, stop and report. If X1's tests fail to compile or fail in your check, fix `src/orchestrator/core/manual_start.rs` for that alone (X1 has returned; no other worker writes it), keeping its pinned signatures and the four copied lines of each builder byte for byte, and say so in your report. Also read only: `src/orchestrator/terminal/native/wrapper.rs` (`exec env -i "${_loom_env[@]}"`: the environment allowlist), `src/orchestrator/terminal/native/session_settings/contents.rs` (`capsule_settings`: the capsule carries the sandbox block, write denies and hooks).

The pinned interface X1 wrote, in `orchestrator::core::manual_start`:

```rust
pub(super) fn manual_stage_start_lines(stage_id: &str, worktree: &Path, session_id: &str, signal: &Path, work_dir: &Path) -> Vec<String>
pub(super) fn manual_knowledge_start_lines(stage_id: &str, repo_root: &Path, signal: &Path) -> Vec<String>
```

## Steps

1. **`mod.rs`.** Add `mod manual_start;` in the alphabetical module list, between `mod lifecycle_snapshot;` and `mod merge_handler;`.
2. **`stage_spawn.rs`.** `print_manual_start` keeps its signature and doc comment; its body becomes a loop that prints each line of `super::manual_start::manual_stage_start_lines(stage_id, &worktree.path, &session.id, signal_path, work_dir)` with `println!("{line}")`. Call both builders by that `super::manual_start::` path, with no `use` line: the stage's wiring checks grep `manual_start::manual_stage_start_lines(` and `manual_start::manual_knowledge_start_lines(`. The builder canonicalizes `work_dir` itself. The file is 398 lines and must end at or under 400.
3. **`stage_knowledge.rs`.** The four `println!` calls of the `if self.config.manual_mode` branch become the same loop over `super::manual_start::manual_knowledge_start_lines(&stage.id, &self.config.repo_root, signal)`; `return Some(session);` stays.
4. **`../doc/sandbox-release-checklist.md`.** Title `# Sandbox Release Checklist`. Content, as current-truth prose and one table:
   - Why it exists: CI has no credentialed Claude Code runtime. The capsule's denies are proven by unit tests on the emitted capsule, by the bubblewrap probe `tests_capsule_denies::bwrap` (CI runs it in its own step with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, so a skipped probe fails there), and by this checklist, which a person runs against a live, credentialed session.
   - When: before tagging a release that changes `loom/src/orchestrator/terminal/native/session_settings/`, `loom/src/sandbox/`, `loom/src/fs/permissions/state_root.rs` or `loom/src/git/worktree/config_worktree.rs`, and after a Claude Code upgrade.
   - Scope: Linux only. On macOS Claude Code's sandbox is Seatbelt, which this checklist does not exercise; a release records that it was not run there.
   - Setup: a Linux host with `bwrap`; install the candidate with `./dev-install.sh`; in a scratch git repository outside any project (one commit), a scratch plan with two standard stages `probe-a` and `probe-b`, no dependency between them, acceptance `true`, descriptions telling the agent to run only the commands the operator gives it; a third stage `probe-u` with `ultracode: true`; the integration-verify and knowledge-distill bookends the plan lint requires, each with acceptance `true`; when the codex CLI and plugin are installed, `probe-b` lists `implementers: ["claude", "codex"]`. Check it with `loom plan verify --strict`, then `loom init` and `loom run`. `R` below is the scratch repository's root. Commit the scratch plan before `loom run`, which refuses an uncommitted plan. Record `claude --version` and `bwrap --version` at the top of the results: the expected results below were measured with Claude Code 2.1.306 and bubblewrap 0.11.1. Before row 1, run row 1's loop once on the host: the files that exist there are the ones rows 1 and 3 prove, and the rest are recorded as not covered.
   - The probe table (columns: Probe, Run in, Command or action, Expected). The first column holds the row number, written `| 1 |` to `| 15 |`: the stage's acceptance counts those rows. No cell may contain a raw `|`. One row each:
     1. Credential files from Bash, in `probe-a`: `for f in ~/.config/gh/hosts.yml ~/.netrc ~/.npmrc ~/.git-credentials ~/.docker/config.json ~/.kube/config ~/.pypirc ~/.cargo/credentials.toml ~/.cargo/credentials ~/.codex/auth.json; do [ -e "$f" ] && { cat "$f" >/dev/null 2>&1; printf '%s rc=%s\n' "$f" "$?"; }; done`. Expected: every file listed prints a non-zero rc: `cat` gets `Permission denied`, because Claude Code binds each file from `/dev/null` on a `nodev` mount. Never print a credential's content.
     2. Codex credential in a codex-licensed stage, in `probe-b` (only with codex installed): `cat ~/.codex/auth.json >/dev/null; echo rc=$?`. Expected: `rc=0`.
     3. Interpreter, in `probe-a`: `python3 -c 'import os; open(os.path.expanduser("~/.netrc"), "rb").read()'` (or any other listed file that exists on the host). Expected: `PermissionError`.
     4. Build script, in `probe-a`: a scratch crate under `$TMPDIR` whose `build.rs` emits `cargo:warning=` lines with the error kind of `std::fs::read` on `~/.npmrc` and the error from creating `R/.git/worktrees/probe-b/x`; `cargo build`. Expected: `PermissionDenied`, and `Read-only file system`.
     5. Symlink swap, in `probe-a`: `ln -s ~/.netrc "${TMPDIR:?}/n" && cat "${TMPDIR:?}/n" >/dev/null; echo rc=$?`, then `ln -s R/.git/worktrees/probe-b "${TMPDIR:?}/w" && touch "${TMPDIR:?}/w/x"`. Expected: a non-zero rc (`Permission denied`), then `Read-only file system`.
     6. Sibling administrative directory, in `probe-a`: `touch R/.git/worktrees/probe-b/x`. Expected: `Read-only file system`.
     7. Own administrative directory, in `probe-a`: create a file, `git add` it, `git status`, then `git restore --staged` it. Expected: all succeed.
     8. A sibling merges mid-session: tell `probe-b`'s agent to finish its stage; after loom merges it and removes its worktree, run `true` and `git status` in `probe-a`. Expected: both succeed, and on the host `ls -a R/.git/worktrees` shows no `probe-b` entry once the command returns. Do not assert that the directory stays absent inside the sandbox during a command that was already running when it merged: a directory the host deletes vanishes inside that command, which can recreate it.
     9. Refs (open gap: ref denies are out of scope while sessions write the object store), in `probe-a`: `git update-ref refs/heads/release-probe HEAD`. Expected: succeeds. Delete it from the host with `git update-ref -d refs/heads/release-probe`.
     10. File tools, in `probe-a`: ask the agent to Read `~/.netrc`. Expected: refused by `worktree-file-guard.sh` (the path is outside the worktree). Open gap: a checkout-rooted session's file tools are not checked against the credential list.
     11. Tool routes, in `probe-a`: ask the agent to call LSP, CronCreate, RemoteTrigger, PushNotification and Workflow once each, then ListAgents. Expected: each of the first five is refused by a permission rule or not offered at all; ListAgents works.
     12. Workflow in an ultracode stage, in `probe-u`: ask the agent whether the Workflow tool is available and to start an empty workflow. Expected: available.
     13. Manual mode, in a second scratch checkout: `loom init <plan> && loom run --manual --foreground` (without `--foreground` the daemon writes these lines to `.loom/work/orchestrator.log`). Expected: after each line whose text starts with `To start:` comes a line whose text starts with `Confinement: none.`
     14. Sibling worktree reads (open gap: no plan denies them), in `probe-a`: `ls R/.worktrees/probe-b`. Expected: the listing succeeds; record it.
     15. Tools left allowed, in `probe-a`: ask the agent whether Monitor, EnterWorktree and ExitWorktree are offered (never call EnterWorktree in a stage session), then to start a Monitor whose command is `touch R/.git/worktrees/probe-b/m`. Expected: record which tools are offered; the Monitor command fails with `Read-only file system` when Monitor runs under the Bash sandbox. A success is an open route: record it in `doc/loom/knowledge/concerns/sandbox-and-confinement-gaps.md`.
   - A short "Measurement for a later plan" paragraph: before any plan denies sibling worktree reads, run a scratch stage whose plan `sandbox.filesystem.deny_read` lists the absolute path `R/.worktrees`, and confirm the session still reads and writes its own worktree while `ls R/.worktrees` shows nothing else.
   - Recording: each row's result goes in the release pull request; a failed row blocks the release and becomes an entry in `doc/loom/knowledge/concerns/sandbox-and-confinement-gaps.md`.
5. **`../CONTRIBUTING.md`**, in "### Releases" after its first paragraph, add: "A release that changes the session capsule (`loom/src/orchestrator/terminal/native/session_settings/`, `loom/src/sandbox/`) first passes the [sandbox release checklist](doc/sandbox-release-checklist.md): CI has no credentialed Claude Code runtime, so those denies are checked by hand against a live session."
6. **`../README.md`**, "## Sandbox Configuration". Anchor every edit by its text: plan 3 and host verification edit the "What confinement adds" blockquote under "### Command Confinement" in this same section before this stage (host verification and gate-dedup also edit the completion and gate paragraphs), and other plans edit other README sections. Three edits, nothing else:
   - The YAML example lists `"../../**"` and `"../.worktrees/**"` under `deny_read` and `"../../**"` under `deny_write`; none reaches the OS sandbox (`sandbox/settings/policy.rs`: `deny_read_patterns` and `filesystem_settings` drop every entry holding `../`). Remove the two `deny_read` lines and replace the `deny_write` entry with `"vendor/**"`. Anchor each edit by its list key, since `"../../**"` appears under both.
   - In the "Note:" paragraph below the example, replace its last sentence ("A credentialed Claude host-runtime canary ... remains a manual release check.") with: "Every session capsule also read-denies, for Bash and every command it starts, the common credential files (`~/.netrc`, `~/.npmrc`, `~/.git-credentials`, `~/.pypirc`, and the gh, docker, kube and cargo token files), and `~/.codex/auth.json` unless the stage licenses the codex lane; such a file fails to open. The file tools are outside that list: in a worktree `worktree-file-guard.sh` refuses paths outside it, and a checkout-rooted session's file tools are not checked against these files. The capsule also denies the tools that run outside the Bash sandbox: LSP, RemoteTrigger, CronCreate, PushNotification, and Workflow everywhere except the stage session of an `ultracode` stage. `deny_read` and `deny_write` entries holding `../` never reach the OS sandbox. A credentialed check of these denies against a live Claude Code session is the manual [sandbox release checklist](doc/sandbox-release-checklist.md)."
   - "### Session State Confinement", first paragraph: replace "the hooks directories, and git hooks and config, regardless of the filesystem rules above" with "the hooks directories, git hooks and config, and the git administrative directories of other stage worktrees, regardless of the filesystem rules above".

## Traps

- The two print sites must keep their current text exactly; only the notice line is new, and X1's builders produce it.
- Rule 19 applies to every line you write in the checklist and the docs.
- `markdownlint-cli2` runs on these three files and `skills/loom-refactoring/SKILL.md` in the stage's acceptance (read-only, from the repository root), which also counts the checklist's fifteen numbered rows and greps it for `Permission denied` and `Measurement for a later plan`; write tables with a header separator row and fenced blocks with a language.

## Your one check

`cargo test --lib orchestrator::core::manual_start::` from `loom/`, after `cargo fmt`: it runs X1's tests, which is that unit's proof. Report the summary line and `wc -l` of `stage_spawn.rs`.
