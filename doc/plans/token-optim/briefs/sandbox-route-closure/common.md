# Sandbox route closure: facts every worker reads first

Plan: `doc/plans/token-optim/PLAN-06-sandbox-route-closure.md` (renamed with an `IN_PROGRESS-` prefix once `loom run` starts). The plan's YAML is authoritative where this file and the YAML differ. Paths below are relative to `loom/` unless they start with `../` or name a repository-root file.

## What the stage changes

1. **Tool routes.** Every session capsule denies, as bare tool names in `permissions.deny`, the tools that run in the Claude Code process outside the Bash sandbox: `LSP`, `RemoteTrigger`, `CronCreate`, `PushNotification`, and `Workflow` unless the session is the Stage session of an `ultracode: true` stage.
2. **Credential files.** Every capsule's `sandbox.filesystem.denyRead` gains nine plain credential file paths, and `~/.codex/auth.json` unless the stage licenses the codex lane.
3. **Sibling worktree admin directories.** Every capsule write-denies, as whole directories, the administrative directories `<common dir>/worktrees/<name>` of every worktree except the session's own. The session's own directory, `refs/` and the object store stay writable.
4. **Manual mode.** Each manual-mode print site adds one line saying the session it describes runs without the wrapper's confinement.
5. **Proof.** Capsule unit tests, a bubblewrap probe, two contract tests, and a manual release checklist for the live, credentialed checks no test can make.

Out of scope, never added by any worker: ref or common-dir denies, a `Read(` permission rule of any shape, `--disallowedTools`, a deny on `WebFetch`, `WebSearch`, `SendMessage` or `ListAgents`, a deny on Monitor, AppifactRepl, the self-hosted-runner tools, `EnterWorktree` or `ExitWorktree`, sibling worktree read denies, changes to `loom-hooks/credential-guard.sh`, and any edit to `orchestrator/terminal/native/tests_confinement_e2e.rs` (398 lines).

## Workers and lanes

| Worker | Lane | Territory | When |
| --- | --- | --- | --- |
| W1 | Claude, sonnet | tool-route denies and the Workflow license | wave 1 |
| W2 | Claude, haiku | credential read denies | wave 1 |
| W3 | Claude, opus | sibling administrative directory denies | wave 1 |
| X1 | codex, gpt-5.6-terra | the manual-mode line builders, `src/orchestrator/core/manual_start.rs` | wave 1, foreground, after W1-W3 are spawned |
| W4 | Claude, sonnet | manual-mode call sites, module declaration, release docs | after X1 returns |
| W5 | Claude, sonnet | bubblewrap probe | wave 2, after W1-W4 return |

X1 is the one codex unit: a new module with pinned signatures and pure unit tests. Every other piece edits several files together or converges by running fixture-backed tests (real `git worktree add`, `TempDir` capsules, bubblewrap), which a codex unit cannot run. If codex is unavailable at run time or X1 fails, W4 also takes `doc/plans/token-optim/briefs/sandbox-route-closure/route-closure/x1-manual-start.md` and the file it assigns.

## Facts measured or read while planning (2026-10-18/19 at 6fee4ecd; re-checked 2026-10-21 at 50aa8336)

- **bubblewrap 0.11.1 refuses a mount point beneath a read-only bind.** `bwrap --bind / / --dev /dev --ro-bind D D --ro-bind /dev/null D/absent true` exits 1 with `Can't create file at D/absent: Read-only file system`. The reverse order (absent file first, then `D`) runs, and leaves a 0-byte file on the host. A bind over a file that already exists inside `D` runs in either order. Rule that follows: loom never emits a `denyWrite` entry beneath another `denyWrite` directory it emits. bubblewrap mounts every non-dev bind `nodev`, so a file bound with `--ro-bind /dev/null <p>` fails to open with EACCES (`Permission denied`); it does not read empty (measured 2026-10-21, also nested inside a host-job-style `bwrap --unshare-all ... --remount-ro /`; only `--dev-bind` reads 0 bytes). `--tmpfs` on an absent path beneath a read-only bind aborts the same way.
- **Claude Code (measured on 2.1.302, re-read on 2.1.306, the installed version on 2026-10-21)** (2.1.302 bundle at `~/.local/share/claude/versions/2.1.302`) creates an empty `config.worktree` with an exclusive create in the git common directory and in every `worktrees/<name>/` directory entry, then denies it and that entry's `commondir`. An absent `config.worktree.lock` goes to a scrub list, not a bind. These built-in denies are pushed before the settings sources' `denyWrite` entries. Read from minified bundle strings, never observed live. 2.1.306 also binds a read-denied file with `--ro-bind /dev/null <p>`; skips an existing deny path that lies under an already read-only denied directory; warns when a read-deny placeholder would need a mount point under a read-only deny; defers the `config.worktree` create only in remote mode and runs it through symlinked registry entries too; and mounts a placeholder for an absent deny path on later commands. The no-nesting rule below stays: 2.1.306 guards only the cases it predicts.
- **The srt emulation runs the other order.** `tests_confinement_srt.rs` `srt_settings` appends its emulation of Claude Code's git denies (`claude_code_git_denies`, which lists `worktrees/*/commondir` whether or not it exists) after the capsule's `denyWrite`, and the e2e fixture (`tests_confinement_e2e.rs` `repo_with_worktree`) writes admin directories with no `commondir`. With a sibling directory denied whole, that order puts an absent-file bind beneath a read-only directory bind. W3 moves the emulated denies first, which is Claude Code's order. These e2e tests skip without `srt` and `socat`; CI installs neither.
- **`--disallowedTools` is variadic** (`claude --help`: `--disallowedTools, --disallowed-tools <tools...>`; measured on 2.1.302, holds on 2.1.306), and `build_claude_command` (`orchestrator/terminal/native/mod.rs`, the `format!` at about line 174) puts the positional prompt right after the capsule flags, so the flag would swallow the prompt. `build_claude_command_capsule_flags_precede_positional_prompt` (`tests_capsule.rs`, about lines 83-103) checks only string order. Tool denies go in `permissions.deny`.
- **Tools that stay.** `SendMessage` and `ListAgents` serve agent teams (`orchestrator/terminal/native/wrapper.rs`, the `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` line near 363). `WebFetch` and `WebSearch` are in the `tools:` lines of `agents/loom-software-engineer.md` and `agents/loom-senior-software-engineer.md`, and their sandbox bypass is unverified.
- **Code-executing tools this stage leaves allowed.** Claude Code 2.1.306 flags Bash, PowerShell, Workflow, CronCreate, RemoteTrigger, Monitor, AppifactRepl (the bundle's spelling) and two self-hosted-runner tools with `enablesCodeExecution`; `EnterWorktree` and `ExitWorktree` create and remove worktrees. The stage denies only LSP, RemoteTrigger, CronCreate, PushNotification and Workflow; whether the others run under the Bash sandbox is unmeasured, so they stay allowed (plan deviation 10) and release checklist row 15 measures Monitor and the worktree tools.
- **Workflow is a sanctioned feature for ultracode stages.** `launch.rs` `initial_prompt` (about lines 130-145) licenses the Workflow tool by putting the keyword "ultracode" in the Stage session's prompt when `stage.ultracode`; `orchestrator/signals/format/sections.rs` (about line 263) tells that session to use the Workflow tool. A blanket `Workflow` deny would break every ultracode stage.
- **Never a `Read(` deny.** Claude Code prompts on every relative-path `rg`, `grep`, `diff`, `git`, `cp` and `mv` once any `Read` or `Read(...)` entry exists under `permissions.deny` (knowledge: `concerns/sandbox-and-confinement-gaps.md`, "No `Read(...)` Deny Rule May Exist in Any Settings File"). Bare tool names do not trip that predicate (`r === "Read" || r.startsWith("Read(")`).
- **How a `denyRead` entry binds on Linux:** a file is bound from `/dev/null` on a `nodev` mount, so opening it fails with EACCES, a directory is covered by a tmpfs, an absent path mounts nothing, and a glob expands to one mount per match. A plain path costs one mount.
- **Relative `../` deny_read entries never reach the OS sandbox**: `sandbox/settings/policy.rs` `deny_read_patterns` filters every entry containing `../` (about line 46).
- **An opaque iterator of `&'static str` does not chain onto borrowed items.** `deny_read_patterns` chains the mandatory list onto `config.filesystem.deny_read.iter().map(String::as_str)`, whose items borrow the config. A helper returning `impl Iterator<Item = &'static str>` fails there with E0521 ("borrowed data escapes outside of function"); one returning `Vec<&'static str>` compiles. Measured with `rustc --edition 2021` on a ten-line program.
- **Where sessions run.** Stage, Contract and Merge sessions start at the stage worktree root (`orchestrator/terminal/backend.rs` `spawn_worktree_session`; `native/mod.rs` `spawn` records the worktree path), and the merge resolver merges in its own worktree (`orchestrator/signals/merge.rs` `format_merge_task`: `git merge --no-commit --no-ff`). Knowledge and BaseConflict sessions run in the checkout. An Adjudication judge runs in the disputed stage's worktree while `.worktrees/<id>` is a directory, else in the checkout (`orchestrator/adjudication/session.rs` `judge_cwd`); `tests_capsule_denies.rs` `kinds_with_cwd` models only that fallback. Whether a capsule is worktree-rooted follows its cwd (`sandbox/settings.rs` `target_is_worktree`), never its kind.
- **Commits are daemon-owned.** Sessions run `git add` (writes the object store and their own admin directory's `index`) and `loom stage commit`; the daemon writes the commit and moves the ref (`git/stage_commit.rs`, `orchestrator/core/inbox_drain/commit.rs`). No session needs to write another worktree's admin directory.
- **The shared capsule fixture.** `tests_session_settings.rs` `Checkout` exposes `repo`, `worktree`, `hooks_dir` and `home` as `pub(super)` and keeps `_temp`, `work_dir`, `scratch_root` and `tmp_root` private; `checkout()` creates no git repository, so a capsule built on it has no administrative directories unless a test runs `register_worktrees`.
- **The existing bubblewrap skip helper.** `src/process/sandbox_probe.rs` has `confined_commands_runnable()` (a memoized `bwrap --unshare-all ... true`) and `skip_unless(probe_ok, test_name, why)`, which prints `SKIP` and returns true, or panics under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`. `scripts/rust_test_environment.py` passes only that flag and `LOOM_E2E_REQUIRE_TMUX` through `scripts/run-rust-tests.py` and strips every other `LOOM_*` variable, so a test that needs bubblewrap uses these two and no new flag.

## Size limits (re-measure at run time)

`loom/tests/maintainability/scanner.rs` fails a file over 400 lines or a function over 50 (a function spans its `fn` line to its closing brace; doc comments do not count). `loom/maintainability-baseline.txt` pins the over-limit files and functions exactly and fails on growth and on shrinkage; it is a `ratchet_files` entry of this plan, so no worker changes it. Counts at 50aa8336 (2026-10-21):

| File | Lines | Constraint |
| --- | --- | --- |
| `src/orchestrator/terminal/native/session_settings/contents.rs` | 321 at 50aa8336; about 313 after host verification | `capsule_settings` is at most 50 lines: 42 after host verification removed its macOS daemon-socket block (`contents.rs:72-113`, the deny line about `:97`). Replace lines one for one, never add |
| `src/orchestrator/terminal/native/session_settings.rs` | 304 | |
| `src/orchestrator/terminal/native/launch.rs` | 366 | `prepare_session_launch_with` is 46 lines |
| `src/orchestrator/terminal/native/tests_session_settings.rs` | 398 | at most one added line; host verification removes about five |
| `src/orchestrator/terminal/native/tests_capsule_contents.rs` | 382 | |
| `src/orchestrator/terminal/native/tests_capsule.rs` | 390 | at most two added lines |
| `src/orchestrator/terminal/native/tests_launch_capsule.rs` | 357 | the new test reuses `try_launch` through a split helper; never copy its body |
| `src/orchestrator/terminal/native/tests_capsule_denies.rs` | 127 | |
| `src/orchestrator/terminal/native/tests_confinement_srt.rs` | 352 | |
| `src/orchestrator/terminal/native/tests_confinement_e2e.rs` | 398 | never edit |
| `src/sandbox/control_surfaces/session_denies.rs` | 244 | `session_denies` is 46 lines |
| `src/sandbox/control_surfaces/tests_session_denies.rs` | 320 | new tests go in a new file |
| `src/sandbox/settings/policy.rs` | 183 at 50aa8336; 176 after host verification removes its opt-out block | `filesystem_settings` is ledgered at exactly 52: do not edit it |
| `src/sandbox/settings/tests.rs` | 1565 | ledgered at exactly 1565: edit lines in place, never add or remove one |
| `src/git/worktree/config_worktree.rs` | 125 | |
| `src/git/worktree/config_worktree_tests.rs` | 160 | |
| `src/orchestrator/core/stage_spawn.rs` | 398 | may shrink, may grow by at most two |
| `src/orchestrator/core/mod.rs` | 347 at 50aa8336; host verification adds `mod host_jobs;`, `mod orchestrator_result;` and a re-export (about 348), so re-measure | |

Run `wc -l` on every file you touch before and after `cargo fmt`, and keep each one at or under its limit.

## Rules for every worker

- Read this file, then your brief, then the ranges your brief names. Anchor every edit by symbol: line numbers above are from 6fee4ecd and drift.
- Plans 3, 1, 2, host verification, 4 and 5 of the token-optimization programme merge before this one. Host verification edits eight files this stage owns: `contents.rs` (removed the macOS daemon-socket block from `capsule_settings`), `sandbox/settings/policy.rs` (removed the `allowNestedUserNamespaces` opt-out), `sandbox/settings/policy/tests.rs` (its last test is now `enabled_sandbox_never_allows_nested_user_namespaces`), `src/orchestrator/core/mod.rs`, `src/sandbox/settings/tests.rs` (line-neutral: `allow.len()` 15 → 17 and the `verification/**` read, in asserts near `:245`, `:253` and `:665`), `src/orchestrator/terminal/native/tests_capsule_contents.rs` (the hook-registration assertions near `:227-228`), `src/orchestrator/terminal/native/tests_session_settings.rs` (the macOS `allowUnixSockets` assertion near `:270-276`), and `../README.md` (the "What confinement adds" blockquote under "### Command Confinement" and the completion and gate paragraphs, which gate-dedup rewrites again). None of these edits touches an anchor a brief uses. No other earlier plan edits a file this stage owns. Re-read each file before editing it and re-run `wc -l`.
- After host verification your shell cannot create user namespaces, so tests that need nested bubblewrap (the probe included) print SKIP in your check; criteria run on the host through `loom check`, where they run for real.
- Never run git, never touch a `.loom/` path, never edit a file outside your brief's ownership list. Other workers edit the crate in parallel: a compile error in a file you do not own is not yours. Report it and stop, and never wait for it.
- Run the one check your brief names, once. The stage's main agent runs the full gate.
- Keep every test name and module path your brief gives verbatim: the stage's acceptance lists four of them (`cargo test --lib -- --list | rg -qF ...`), and the bubblewrap one exists only on Linux, where the stage runs.
- Record mistakes, decisions and surprises with `loom memory note` / `loom memory decision` as they happen; never Claude Code auto-memory.
- No placeholders, no stubs, no `todo!()`. Comments state current behaviour and the constraint behind it, with no history.
