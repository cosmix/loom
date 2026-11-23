# hooks-and-delivery / w3-installers-and-tests: declined installs, PATH hint, agent headings, test-architecture fixes

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/delivery.md`, sections I1, CR-70 (sub-claims a, b, c, d for `web_host.rs`), CR-78 (bullets 2 and 3c).

## Findings covered
- I1: declining the overwrite prompt in `install.sh` exits 0, so `dev-install.sh` goes on to `pkill -x loom`; `./dev-install.sh --help` builds and then kills loom as well.
- CR-78 3c: `install.sh` prints no hint when `$HOME/.local/bin` is not on `PATH`.
- CR-78 bullet 2: `agents/loom-senior-software-engineer.md` and `agents/loom-software-engineer.md` use the heading `## Skills to Leverage`.
- CR-70(a, narrowed): `binary_spawn_guard.rs` cannot see an aliased `CARGO_BIN_EXE_loom`.
- CR-70(b): `web_host.rs` strips 11 hardcoded `LOOM_*` names instead of all of them.
- CR-70(c): two tests mutate the process environment without `#[serial]`.
- CR-70(d), `web_host.rs` only: a wildcard-bind skip bypasses `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`.

## Files
Own (write):
- `install.sh`, `dev-install.sh`
- `agents/loom-senior-software-engineer.md`, `agents/loom-software-engineer.md`
- `scripts/tests/test_install.py`, `scripts/tests/test_dev_install.py`
- `loom/tests/web_host.rs`, `loom/tests/summary_engine_contracts.rs`, `loom/tests/integration/binary_spawn_guard.rs`
- `loom/src/commands/plan/verify/dry_run_real_tests.rs`
Read only: `loom/tests/test_support/mod.rs` (`clear_loom_env`), `loom/tests/worktree_cleanup_guidance.rs` :1-10 (how a top-level test target includes `test_support`), `loom/src/process/sandbox_probe.rs` (`skip_unless`, :338), `loom/tests/planning_summary_cli_contracts.rs` :20 (`use serial_test::serial;` in an integration target).
Frozen (never edit): `loom/tests/installer_decline_contracts.rs`.
No shell test is added under `loom-hooks/tests/`; none is needed.

## Changes

### 1. I1 - declined install
- `install.sh`, function `confirm_overwrites`: on an answer other than y/Y keep the `info "cancelled"` line and change `exit 0` to `exit 1`. No caller reads the code (`main` calls it on the checkout path only), so no constant is added.
- `dev-install.sh`: above the `pgrep -x loom` block add a comment that a declined install exits non-zero and `set -e` stops the script before the kill.
- `--help`: right after the `SCRIPT_DIR=` line add a loop over `"$@"`; on `-h` or `--help` run `exec bash "$SCRIPT_DIR/install.sh" "$@"`. The usage text prints, nothing is built, nothing is killed.

### 2. CR-78 3c - PATH hint
`install.sh`, function `print_summary`: after the `~/.local/bin/` block and before the "next steps" lines, add `case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) warn "..." ;; esac`, with the text `$HOME/.local/bin is not on your PATH; add it with: export PATH="$HOME/.local/bin:$PATH"` (escape so the printed text shows literal `$HOME` and `$PATH`). Use the existing `warn` helper (:75).

### 3. CR-78 bullet 2 - agent headings
Rename `## Skills to Leverage` to `## Skills to Use` in both agent files (`loom-senior-software-engineer.md:22`, `loom-software-engineer.md:29`). Change nothing else in them. Nothing in the tree anchors on the old heading (`rg 'Skills to Leverage'` finds only these two).

### 4. CR-70(b) and (d) - `loom/tests/web_host.rs`
- Add `#[path = "test_support/mod.rs"] mod test_support;` after the imports. In `loom_cmd` replace the `for var in RELAY_ENV_VARS` loop with `test_support::clear_loom_env(&mut command);` (before `command.env("LOOM_HOME", home)` - keep the existing `LOOM_HOME` set after the clear). Delete the `RELAY_ENV_VARS` const and its doc comment.
- (d): in `remote_wildcard_dashboard_serves_viewing_without_the_cookie`, replace the `eprintln!("SKIP ...")` and `return` in the wildcard-bind `Err` arm with `if loom::process::sandbox_probe::skip_unless(false, NAME, &format!("wildcard bind unavailable: {error}")) { return; }`.
- Add `use serial_test::serial;` and mark all three tests in the file `#[serial]` (the new one sets a process variable; the others spawn children that read the environment).

### 5. CR-70(c) - `#[serial]`
- `loom/src/commands/plan/verify/dry_run_real_tests.rs`: `use serial_test::serial;` and `#[serial]` on `the_plans_inherit_confinement_reaches_acceptance` (it sets `LOOM_DRY_RUN_PROBE` through `EnvGuard`).
- `loom/tests/summary_engine_contracts.rs`: `use serial_test::serial;` and `#[serial]` on `summary_models_default_to_haiku_for_documents_and_sonnet_for_overviews` (`empty_user_config` sets `LOOM_HOME`).

### 6. CR-70(a, narrowed) - `loom/tests/integration/binary_spawn_guard.rs`
Today only the inline spawn forms are flagged. Add a second check on the bare handle:
- A function `fn mentions_binary_handle(content: &str) -> bool` testing for the handle name `CARGO_BIN_EXE_loom`, built from parts at runtime (`["CARGO_BIN_", "EXE_loom"].concat()`), as `forbidden_patterns` does, so this file's own code holds no literal.
- A const `NON_SPAWNING_REFERENCES: &[(&str, &str)]` of (path under `loom/tests/`, reason). Open each file and confirm the reason before listing it:
  - `plan_session_sandbox_contracts/support.rs`: alias; spawns only through `loom_cmd`, which scrubs every `LOOM_*`
  - `plan_session_confinement/fixture.rs`: alias; the scrub is done by `Confined::run`
  - `worktree_cleanup_guidance.rs`: let binding; its `run` scrubs `LOOM_*`
  - `planning_summary_seatbelt.rs`: scrubs before its spawn
  - `plan_redesign_contracts.rs`, `plan_session_bridge_e2e.rs`: path handed to library code, no direct spawn
  - `host_verification_contracts/support.rs`: spawns through `helpers::loom_cmd`
  - `integration/hooks_skill_trigger_fixture.rs`, `integration/hooks_skill_project.rs`: `LOOM_BIN` hand-off for a hook script, no spawn
  A file that turns out to spawn without a full scrub is reported to the orchestrator, not listed and not edited.
- Add `"web_host.rs"` to `SANCTIONED_SPAWNERS` (its `loom_cmd` now scrubs through `clear_loom_env`) and extend the doc comment list.
- New test `every_use_of_the_binary_handle_is_accounted_for`: over `rust_files_under(tests_dir)`, skip this file (`integration/binary_spawn_guard.rs`); every file mentioning the handle must be in `SANCTIONED_SPAWNERS` or `NON_SPAWNING_REFERENCES`; every `NON_SPAWNING_REFERENCES` entry must exist and still mention the handle (a stale entry fails).
- New test `an_aliased_handle_is_a_mention`: `mentions_binary_handle` is true for a text holding `const LOOM: &str = env!("<handle>");` (build the text with `format!`/`concat`) and false for `const LOOM: &str = "loom";`.

## Interfaces shared with other workers
None. `loom/tests/installer_decline_contracts.rs` (frozen) sources `install.sh` with `LOOM_INSTALL_LIB_ONLY=1` and calls `confirm_overwrites`.

## Tests
- `scripts/tests/test_install.py`: change `run_install(self, body, **overrides)` to `run_install(self, body, input_text=None, **overrides)` and pass `input=input_text` to `subprocess.run`. New tests: (1) `test_declined_overwrite_exits_nonzero`: `(self.root / ".claude/agents").mkdir(parents=True)`, body `confirm_overwrites`, `input_text="n\n"`; assert `returncode != 0` and `"cancelled" in stdout` (fails today: returncode 0); control with `"y\n"` returns 0. (2) `test_summary_warns_when_local_bin_is_not_on_path`: body `print_summary`, `PATH="/usr/bin:/bin"`; assert `export PATH=` in stdout; and with `PATH=f"{self.root}/.local/bin:/usr/bin:/bin"` it is absent. Fails today for the first.
- `scripts/tests/test_dev_install.py`: let `run_install` take `args=("--skills", "all")`. New `test_help_neither_builds_nor_signals`: `args=("--help",)`; assert returncode 0, `build.json` absent, `signals.trace` absent, `install.trace` holds `--help`. The wrapper stopping on a non-zero install is already covered by `test_failed_install_does_not_signal` (INSTALL_EXIT=9); add no second test for it.
- `loom/tests/web_host.rs`: new `#[serial]` test `loom_cmd_clears_every_loom_variable`: `std::env::set_var("LOOM_ADMIN_SECRET", "sentinel")` (restore afterwards), build `loom_cmd(base, home, &[])` from `workspace()`, assert `command.get_envs()` holds `("LOOM_ADMIN_SECRET", None)` and `LOOM_HOME` is `Some`. Fails today (the sentinel is not in the 11-name list).
- `loom/tests/integration/binary_spawn_guard.rs`: the two tests of item 6.
- Contract to turn green: `declined_install_exits_non_zero` (file `loom/tests/installer_decline_contracts.rs`).
- Existing test whose assertions change: `binary_spawn_guard.rs::only_sanctioned_spawners_spawn_the_loom_binary_directly` gains the `web_host.rs` row in `SANCTIONED_SPAWNERS`; its scan logic is unchanged. Report it.

## Proof
`bash -n install.sh dev-install.sh`; `python3 -B -m unittest discover -s scripts/tests -p test_install.py` and `-p test_dev_install.py`; from `loom/`: `cargo test --test web_host`, `cargo test --test summary_engine_contracts`, `cargo test --test integration binary_spawn_guard`, `cargo test --lib commands::plan::verify::dry_run::dry_run_real_tests::` (these skip where bubblewrap is refused; the check is that it compiles and does not fail). `rg 'Skills to Leverage' agents` returns nothing.

## Traps
- `confirm_overwrites` with `read -r` at EOF fails under `set -e` anyway (non-zero); the regression test must feed an explicit `n`, or it passes for the wrong reason.
- `exit 1` must stay inside the existing `if`; do not move `info "cancelled"` after it.
- `dev-install.sh --help` must `exec` before `cd "$SCRIPT_DIR/loom"` and before the build; a later check still builds first.
- In `web_host.rs`, `LOOM_HOME` must be set after `clear_loom_env`, or the new scrub removes it and the child reads the operator's real config.
- `clear_loom_env` scrubs only variables present in the parent: the new test must set the sentinel before building the command.
- `std::env::set_var` in a test without `#[serial]` races the other tests' spawns; all three tests in `web_host.rs` get the attribute.
- The handle literal must not appear in `binary_spawn_guard.rs` code except through the built-from-parts form, or the new test flags the file itself; the file is excluded by path regardless, but keep the pattern.
- `loom/tests/integration/hooks_skill_project.rs` and `hooks_skill_trigger_fixture.rs` only pass the binary path as `LOOM_BIN`: they are references, not spawners.
