# Sandbox Release Checklist

CI has no credentialed Claude Code runtime. The capsule's denies are proven by unit tests on the emitted capsule, by the bubblewrap probe `tests_capsule_denies::bwrap` (CI runs it in its own step with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, so a skipped probe fails there), and by this checklist, which a person runs against a live, credentialed session.

## When to run it

Before tagging a release that changes `loom/src/orchestrator/terminal/native/session_settings/`, `loom/src/sandbox/`, `loom/src/fs/permissions/state_root.rs` or `loom/src/git/worktree/config_worktree.rs`, and after a Claude Code upgrade.

## Scope

Linux only. On macOS Claude Code's sandbox is Seatbelt, which this checklist does not exercise; a release records that it was not run there.

## Setup

- A Linux host with `bwrap`. Install the candidate with `./dev-install.sh`.
- A scratch git repository outside any project, with one commit. `R` below is its root.
- A scratch plan with two standard stages `probe-a` and `probe-b`, no dependency between them, acceptance `true`, and descriptions telling the agent to run only the commands the operator gives it.
- A third stage `probe-u` with `ultracode: true`.
- The integration-verify and knowledge-distill bookends the plan lint requires, each with acceptance `true`.
- When the codex CLI and plugin are installed, `probe-b` lists `implementers: ["claude", "codex"]`.
- Check the plan with `loom plan verify --strict`, commit it (`loom run` refuses an uncommitted plan), then run `loom init` and `loom run`.
- Record `claude --version` and `bwrap --version` at the top of the results. The expected results below were measured with Claude Code 2.1.306 and bubblewrap 0.11.1.
- Before row 1, run row 1's loop once on the host: the files that exist there are the ones rows 1 and 3 prove, and the rest are recorded as not covered.

## Probes

| Row | Probe | Run in | Command or action | Expected |
| --- | --- | --- | --- | --- |
| 1 | Credential files from Bash | `probe-a` | `for f in ~/.config/gh/hosts.yml ~/.netrc ~/.npmrc ~/.git-credentials ~/.docker/config.json ~/.kube/config ~/.pypirc ~/.cargo/credentials.toml ~/.cargo/credentials ~/.codex/auth.json; do [ -e "$f" ] && { cat "$f" >/dev/null 2>&1; printf '%s rc=%s\n' "$f" "$?"; }; done` | Every file listed prints a non-zero rc: `cat` gets `Permission denied`, because Claude Code binds each file from `/dev/null` on a `nodev` mount. Never print a credential's content. |
| 2 | Codex credential in a codex-licensed stage (only with codex installed) | `probe-b` | `cat ~/.codex/auth.json >/dev/null; echo rc=$?` | `rc=0` |
| 3 | Interpreter | `probe-a` | `python3 -c 'import os; open(os.path.expanduser("~/.netrc"), "rb").read()'` (or any other listed file that exists on the host) | `PermissionError` |
| 4 | Build script | `probe-a` | A scratch crate under `$TMPDIR` whose `build.rs` emits `cargo:warning=` lines with the error kind of `std::fs::read` on `~/.npmrc` and the error from creating `R/.git/worktrees/probe-b/x`; run `cargo build` | `PermissionDenied`, and `Read-only file system` |
| 5 | Symlink swap | `probe-a` | `ln -s ~/.netrc "${TMPDIR:?}/n" && cat "${TMPDIR:?}/n" >/dev/null; echo rc=$?`, then `ln -s R/.git/worktrees/probe-b "${TMPDIR:?}/w" && touch "${TMPDIR:?}/w/x"` | A non-zero rc (`Permission denied`), then `Read-only file system` |
| 6 | Sibling administrative directory | `probe-a` | `touch R/.git/worktrees/probe-b/x` | `Read-only file system` |
| 7 | Own administrative directory | `probe-a` | Create a file, `git add` it, `git status`, then `git restore --staged` it | All succeed |
| 8 | A sibling merges mid-session | `probe-a` | Tell `probe-b`'s agent to finish its stage; after loom merges it and removes its worktree, run `true` and `git status` in `probe-a` | Both succeed, and on the host `ls -a R/.git/worktrees` shows no `probe-b` entry once the command returns. Do not assert that the directory stays absent inside the sandbox during a command that was already running when it merged: a directory the host deletes vanishes inside that command, which can recreate it. |
| 9 | Refs (open gap: ref denies are out of scope while sessions write the object store) | `probe-a` | `git update-ref refs/heads/release-probe HEAD` | Succeeds. Delete it from the host with `git update-ref -d refs/heads/release-probe`. |
| 10 | File tools | `probe-a` | Ask the agent to Read `~/.netrc` | Refused by `worktree-file-guard.sh` (the path is outside the worktree). Open gap: a checkout-rooted session's file tools are not checked against the credential list. |
| 11 | Tool routes | `probe-a` | Ask the agent to call LSP, CronCreate, RemoteTrigger, PushNotification and Workflow once each, then ListAgents | Each of the first five is refused by a permission rule or not offered at all; ListAgents works. |
| 12 | Workflow in an ultracode stage | `probe-u` | Ask the agent whether the Workflow tool is available and to start an empty workflow | Available |
| 13 | Manual mode | a second scratch checkout | `loom init <plan> && loom run --manual --foreground` (without `--foreground` the daemon writes these lines to `.loom/work/orchestrator.log`) | After each line whose text starts with `To start:` comes a line whose text starts with `Confinement: none.` |
| 14 | Sibling worktree reads (open gap: no plan denies them) | `probe-a` | `ls R/.worktrees/probe-b` | The listing succeeds; record it. |
| 15 | Tools left allowed | `probe-a` | Ask the agent whether Monitor, EnterWorktree and ExitWorktree are offered (never call EnterWorktree in a stage session), then to start a Monitor whose command is `touch R/.git/worktrees/probe-b/m` | Record which tools are offered; the Monitor command fails with `Read-only file system` when Monitor runs under the Bash sandbox. A success is an open route: record it in `doc/loom/knowledge/concerns/sandbox-and-confinement-gaps.md`. |

## Measurement for a later plan

Before any plan denies sibling worktree reads, run a scratch stage whose plan `sandbox.filesystem.deny_read` lists the absolute path `R/.worktrees`, and confirm the session still reads and writes its own worktree while `ls R/.worktrees` shows nothing else.

## Recording

Each row's result goes in the release pull request. A failed row blocks the release and becomes an entry in `doc/loom/knowledge/concerns/sandbox-and-confinement-gaps.md`.
