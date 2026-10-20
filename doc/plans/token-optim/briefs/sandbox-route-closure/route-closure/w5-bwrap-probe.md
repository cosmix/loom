# W5: the bubblewrap probe of the emitted capsule

Tier: sonnet (`loom-software-engineer`). Wave 2: W1-W4 and X1 have returned and the crate builds. Read `doc/plans/token-optim/briefs/sandbox-route-closure/common.md` first.

## Goal

A test that builds a real capsule through `write_session_capsule`, translates its `denyWrite` and `denyRead` lists into bubblewrap binds the way Claude Code's Linux sandbox applies them, runs probes inside, and proves three things: bubblewrap can build the bind set loom emits (a deny nested beneath another deny makes it abort), the sibling worktree's administrative directory refuses a write while the session's own directory, the object store and the worktree take one, and every credential file reads empty while an unlisted file beside it does not. CI runs it for real; elsewhere it skips when bubblewrap cannot start.

## Files you own

- `src/orchestrator/terminal/native/tests_capsule_bwrap.rs` (the main agent created it holding one `//!` line; replace the whole file)
- `../.github/workflows/ci.yml` (one env line)

The file is the child module `bwrap` of `tests_capsule_denies.rs` (declared at that file's end with `#[cfg(target_os = "linux")]` and `#[path = "tests_capsule_bwrap.rs"]`), so `super::register_worktrees` and `super::kinds_with_cwd` are in reach, and `crate::orchestrator::terminal::native::tests_session_settings::{checkout, read_capsule, sandbox_with, strings, write_capsule}` are importable (see the `use` lines at the top of `tests_capsule_denies.rs`). Read only: `src/handoff/completion/content_tests.rs` `sandbox_null_device_preserves_declared_input_precedence` (the bwrap precedent in this crate), `doc/loom/knowledge/mistakes/verification-harness.md` (the sentinel rule) and `doc/loom/knowledge/architecture/execution-containment.md`, section "The Test Pattern That Makes A Boundary Test Able To Fail".

## Steps

1. **Skip gate.** `fn bwrap_runs() -> bool`: `bwrap --ro-bind / / --dev /dev -- true` exits 0. When it does not: if `LOOM_TEST_REQUIRE_BWRAP` is `1`, panic naming the flag; otherwise `eprintln!("SKIP <test>: bubblewrap cannot start here (set LOOM_TEST_REQUIRE_BWRAP=1 to fail instead)")` and return. A stage sandbox may refuse a nested bubblewrap; CI installs it (`.github/workflows/ci.yml`, "Install tmux, ripgrep and bubblewrap").
2. **Fixture.** `checkout()`, then `register_worktrees(&checkout.repo)` (real `git worktree add` of `s1` and `s2`). The fixture root is `checkout.repo.parent()`: the canonical temp directory holding `repo` (with `.worktrees/s1` and `.worktrees/s2` inside it), `home`, `hooks`, `scratch` and `tmp`. `Checkout` keeps its `TempDir` private; `repo`, `worktree`, `hooks_dir` and `home` are `pub(super)`, and W1 made `work_dir`, `scratch_root` and `tmp_root` `pub(super)` in wave 1. Credential fixtures live in the fake home `checkout.home`: create every entry of `crate::fs::permissions::state_root::CREDENTIAL_FILE_DENY_READ_PATHS` and `CODEX_AUTH_DENY_READ_PATH` (expand `~/` against `checkout.home`, create parents) holding the text `canary`, and `checkout.home/.not-a-credential` holding `canary`.
3. **Translation** (document each rule in a comment; it mirrors what common.md records of Claude Code's Linux sandbox, and never touches a host path outside the fixture):
   - `--ro-bind / /`, `--dev /dev`, then `--bind <fixture root> <fixture root>` (the temp directory holding `repo`, the worktrees and `home`: Claude Code's write grants for the cwd and the git common directory).
   - Each `sandbox.filesystem.denyWrite` entry, in capsule order: expand `~/` against `checkout.home`; skip any path outside the fixture root; an existing path gets `--ro-bind <p> <p>`; an absent path whose parent exists gets `--ro-bind /dev/null <p>`; any other path is skipped.
   - Each `sandbox.filesystem.denyRead` entry: expand `~/`; skip glob entries (`*`, `?`, `[`), absent paths and paths outside the fixture root; an existing file gets `--ro-bind /dev/null <p>`, an existing directory `--tmpfs <p>`.
4. **Probes.** One `sh -c` script per capsule that prints `ALIVE` first, then one line `<name> rc=<n>` or `<name> bytes=<n>` per probe. Assert bubblewrap exited 0 and `ALIVE` was printed before reading any probe line, so a bind set bubblewrap refuses fails the test instead of passing it. Two tests:
   - `sibling_admin_dirs_refuse_writes_and_the_own_dir_takes_them`: for the Stage and the Merge kind in `s1` (`kinds_with_cwd` rows), `touch <common>/worktrees/s2/probe` has rc non-zero; `touch <common>/worktrees/s1/probe`, `touch <common>/objects/probe` and `touch <s1>/probe` have rc 0. For the Knowledge kind in the checkout, both `s1` and `s2` refuse. The rc-0 probes are the matched controls: without them a probe that never wrote anything would read as a deny.
   - `credential_files_read_empty_and_an_unlisted_file_does_not`: for a Stage capsule in `s1` with `sandbox_with(vec![Implementer::Claude])`, every credential file reads 0 bytes (`wc -c < <p>`) and `.not-a-credential` reads 6. Then, with `sandbox_with(vec![Implementer::Claude, Implementer::Codex])`, the codex credential reads 6 bytes.
5. **`../.github/workflows/ci.yml`.** In the `test` job's "Run tests" step `env:` block, beside `LOOM_E2E_REQUIRE_TMUX: "1"`, add `LOOM_TEST_REQUIRE_BWRAP: "1"`. CI is the one place the probe must run, and a skip prints only to captured stderr.

## Traps

- Never bind or create anything outside the fixture's temp directory; the real home is never read or written.
- `bwrap` may leave 0-byte placeholder files for `/dev/null` binds on absent paths; they land inside the temp directory and go with it.
- Keep every function under 50 lines and the file under 400.
- The probe complements the capsule unit tests: it checks loom's emitted lists under real binds, not Claude Code's own translation, which only the release checklist checks.

## Your one check

`cargo test --lib tests_capsule_denies::bwrap -- --nocapture` from `loom/`, after `cargo fmt`. Report whether each test ran or printed `SKIP`, and its summary line.
