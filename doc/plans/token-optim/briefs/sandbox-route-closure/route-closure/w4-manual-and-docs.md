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

Read only: `src/orchestrator/core/manual_start.rs`, which X1 wrote. When the codex unit failed or could not run, your prompt also names `doc/plans/token-optim/briefs/sandbox-route-closure/route-closure/x1-manual-start.md`: then you own `src/orchestrator/core/manual_start.rs` too and write it by that brief first. Otherwise, if the file is missing or its signatures differ from the ones below, stop and report. Also read only: `src/orchestrator/terminal/native/wrapper.rs` (`exec env -i "${_loom_env[@]}"`: the environment allowlist), `src/orchestrator/terminal/native/session_settings/contents.rs` (`capsule_settings`: the capsule carries the sandbox block, write denies and hooks).

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
   - Why it exists: CI has no credentialed Claude Code runtime. The capsule's denies are proven by unit tests on the emitted capsule, by the bubblewrap probe `tests_capsule_denies::bwrap` (CI sets `LOOM_TEST_REQUIRE_BWRAP=1`, so a skipped probe fails there), and by this checklist, which a person runs against a live, credentialed session.
   - When: before tagging a release that changes `loom/src/orchestrator/terminal/native/session_settings/`, `loom/src/sandbox/`, `loom/src/fs/permissions/state_root.rs` or `loom/src/git/worktree/config_worktree.rs`, and after a Claude Code upgrade.
   - Setup: a Linux host with `bwrap`; install the candidate with `./dev-install.sh`; in a scratch git repository outside any project (one commit), a scratch plan with two standard stages `probe-a` and `probe-b`, no dependency between them, acceptance `true`, descriptions telling the agent to run only the commands the operator gives it; a third stage `probe-u` with `ultracode: true`; the integration-verify and knowledge-distill bookends the plan lint requires, each with acceptance `true`; when the codex CLI and plugin are installed, `probe-b` lists `implementers: ["claude", "codex"]`. Check it with `loom plan verify --strict`, then `loom init` and `loom run`. `R` below is the scratch repository's root.
   - The probe table (columns: Probe, Run in, Command or action, Expected). One row each:
     1. Credential files from Bash, in `probe-a`: `for f in ~/.config/gh/hosts.yml ~/.netrc ~/.npmrc ~/.git-credentials ~/.docker/config.json ~/.kube/config ~/.pypirc ~/.cargo/credentials.toml ~/.cargo/credentials ~/.codex/auth.json; do [ -e "$f" ] && printf '%s %s\n' "$f" "$(wc -c < "$f")"; done`. Expected: every file listed prints 0. Never `cat` a credential.
     2. Codex credential in a codex-licensed stage, in `probe-b` (only with codex installed): `wc -c < ~/.codex/auth.json`. Expected: a non-zero count.
     3. Interpreter, in `probe-a`: `python3 -c 'import os; print(len(open(os.path.expanduser("~/.netrc"), "rb").read()))'` (or any other listed file that exists). Expected: 0.
     4. Build script, in `probe-a`: a scratch crate under `$TMPDIR` whose `build.rs` emits `cargo:warning=` lines with the byte length of `~/.npmrc` and the error from creating `R/.git/worktrees/probe-b/x`; `cargo build`. Expected: 0, and `Read-only file system`.
     5. Symlink swap, in `probe-a`: `ln -s ~/.netrc "$TMPDIR/n" && wc -c < "$TMPDIR/n"`, then `ln -s R/.git/worktrees/probe-b "$TMPDIR/w" && touch "$TMPDIR/w/x"`. Expected: 0, then `Read-only file system`.
     6. Sibling administrative directory, in `probe-a`: `touch R/.git/worktrees/probe-b/x`. Expected: `Read-only file system`.
     7. Own administrative directory, in `probe-a`: create a file, `git add` it, `git status`, then `git restore --staged` it. Expected: all succeed.
     8. A sibling merges mid-session: tell `probe-b`'s agent to finish its stage; after loom merges it and removes its worktree, run `true` and `git status` in `probe-a`. Expected: both succeed, and on the host `ls -a R/.git/worktrees` shows no `probe-b` entry once the command returns.
     9. Refs (open gap: ref denies are out of scope while sessions write the object store), in `probe-a`: `git update-ref refs/heads/release-probe HEAD`. Expected: succeeds. Delete it from the host with `git update-ref -d refs/heads/release-probe`.
     10. File tools, in `probe-a`: ask the agent to Read `~/.netrc`. Expected: refused by `worktree-file-guard.sh` (the path is outside the worktree). Open gap: a checkout-rooted session's file tools are not checked against the credential list.
     11. Tool routes, in `probe-a`: ask the agent to call LSP, CronCreate, RemoteTrigger, PushNotification and Workflow once each, then ListAgents. Expected: the first five are refused by a permission rule; ListAgents works.
     12. Workflow in an ultracode stage, in `probe-u`: ask the agent whether the Workflow tool is available and to start an empty workflow. Expected: available.
     13. Manual mode, in a second scratch checkout: `loom init <plan> && loom run --manual`. Expected: after each line whose text starts with `To start:` comes a line whose text starts with `Confinement: none.`
     14. Sibling worktree reads (open gap: no plan denies them), in `probe-a`: `ls R/.worktrees/probe-b`. Expected: the listing succeeds; record it.
   - A short "Measurement for a later plan" paragraph: before any plan denies sibling worktree reads, run a scratch stage whose plan `sandbox.filesystem.deny_read` lists the absolute path `R/.worktrees`, and confirm the session still reads and writes its own worktree while `ls R/.worktrees` shows nothing else.
   - Recording: each row's result goes in the release pull request; a failed row blocks the release and becomes an entry in `doc/loom/knowledge/concerns/sandbox-and-confinement-gaps.md`.
5. **`../CONTRIBUTING.md`**, in "### Releases" after its first paragraph, add: "A release that changes the session capsule (`loom/src/orchestrator/terminal/native/session_settings/`, `loom/src/sandbox/`) first passes the [sandbox release checklist](doc/sandbox-release-checklist.md): CI has no credentialed Claude Code runtime, so those denies are checked by hand against a live session."
6. **`../README.md`**, "## Sandbox Configuration". Anchor every edit by its text: plan 3 and host verification edit the "What confinement adds" blockquote under "### Command Confinement" in this same section before this stage (host verification also edits the completion and verifier paragraphs), and other plans edit other README sections. Three edits, nothing else:
   - The YAML example's `deny_read` lists `"../../**"` and `"../.worktrees/**"`, which never reach the OS sandbox (`sandbox/settings/policy.rs` `deny_read_patterns` drops every entry containing `../`); remove those two lines.
   - In the "Note:" paragraph below the example, replace its last sentence ("A credentialed Claude host-runtime canary ... remains a manual release check.") with: "Every session capsule also read-denies the common credential files (`~/.netrc`, `~/.npmrc`, `~/.git-credentials`, `~/.pypirc`, and the gh, docker, kube and cargo token files), and `~/.codex/auth.json` unless the stage licenses the codex lane, and it denies the tools that run outside the Bash sandbox: LSP, RemoteTrigger, CronCreate, PushNotification, and Workflow everywhere except the session of an `ultracode` stage. `deny_read` entries holding `../` never reach the OS sandbox. A credentialed check of these denies against a live Claude Code session is the manual [sandbox release checklist](doc/sandbox-release-checklist.md)."
   - "### Session State Confinement", first paragraph: its list of write denies ("`.loom/`, `.claude/`, `.worktrees/`, the hooks directories, and git hooks and config") gains "the git administrative directories of other stage worktrees" before "regardless of the filesystem rules above".

## Traps

- The two print sites must keep their current text exactly; only the notice line is new, and X1's builders produce it.
- Rule 19 applies to every line you write in the checklist and the docs.
- `bunx markdownlint-cli2` runs on these three files in the stage's acceptance (read-only, from the repository root); write tables with a header separator row and fenced blocks with a language.

## Your one check

`cargo test --lib orchestrator::core::manual_start::` from `loom/`, after `cargo fmt`: it runs X1's tests, which is that unit's proof. Report the summary line and `wc -l` of `stage_spawn.rs`.
