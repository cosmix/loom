# confinement-hardening H1: split the stage host allowlist, hide the live credential

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first. The frozen
contracts are `tests/hardening_contracts.rs` (and `tests/hardening_contracts/**`),
`src/verify/criteria/confine_env_contract_tests.rs` and
`src/fs/permissions/live_credential_contract_tests.rs`; never edit them. The stage
description's PUBLIC SURFACE is authoritative. You run in wave 2; H2a (wave 3) switches
`verify/criteria/confine.rs` to the function you add, so do not edit `confine.rs` or any
`verify/criteria/*` file.

## Why

The open concern "Confined Commands Still Reach a Live Credential Bus"
(`doc/loom/knowledge/concerns/sandbox-and-confinement-gaps.md`) still holds:
`process/environment.rs` has one list, `STAGE_HOST_ENV_ALLOWLIST`, serving both the terminal
spawners (which need a display and a session bus to attach a window) and plan-authored confined
commands (which need neither). `DBUS_SESSION_BUS_ADDRESS` reaches `org.freedesktop.secrets`, a
live credential surface by the same argument that withholds `SSH_AUTH_SOCK`. First confirm the
concern is still open: `rg -n 'DBUS_SESSION_BUS_ADDRESS' src/process/environment.rs`. If plans
08-11 already split it, record `loom memory note "found: env split already landed in
<commit>"`, report it, and do only parts 3-6.

The live profile's credential is a per-project file the daemon's host-job child reads. It goes
into `STATE_ROOT_SECRET_FILES`, the one list every capsule writer and the confined runner derive
their denies from (`token_deny_paths` for stage, merge, knowledge and planning capsules;
`search_ignore_body` and `ripgrep_config_body`; `verify/criteria/confine_protected.rs` for a
knowledge stage's tree). A home-relative path in `CREDENTIAL_FILE_DENY_READ_PATHS` would miss a
relocated `LOOM_HOME`; the state root has no such gap.

## Edits

1. `src/process/environment.rs`
   - Rename `STAGE_HOST_ENV_ALLOWLIST` to `CONFINED_HOST_ENV_ALLOWLIST` and remove exactly
     `DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`, `DBUS_SESSION_BUS_ADDRESS` from it. Everything
     else stays, `XDG_RUNTIME_DIR`, the `TMUX*`, proxy and CA-bundle names included (the plan's
     Decisions record the revisit condition).
   - Add `pub const TERMINAL_ONLY_ENV_NAMES: &[&str] = &["DISPLAY", "WAYLAND_DISPLAY",
     "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS"];` with a doc comment: a terminal window needs a
     display and the session bus to attach; a confined command needs neither, and the bus
     reaches the secret service.
   - Keep `apply_stage_environment` / `apply_stage_environment_from` with their signatures and
     behaviour (the confined list plus `TERMINAL_ONLY_ENV_NAMES`): the native spawner
     (`orchestrator/terminal/native/spawner.rs`) and `orchestrator/terminal/tmux/mod.rs` keep
     calling them. Their doc comments say they are the terminal spawners' list.
   - Add `pub fn apply_confined_host_environment(command: &mut Command)` and
     `pub fn apply_confined_host_environment_from<I, K, V>(command: &mut Command, source: I)`
     (same generic bounds as `apply_stage_environment_from`): `env_clear()`, then only
     `CONFINED_HOST_ENV_ALLOWLIST` names. Two predicates over the two lists; no name listed
     twice.
   - Move the "This list also governs plan-authored commands ... `cargo`" paragraph to the
     confined list's doc comment.
   - The inline test `every_agent_session_name_is_in_the_stage_host_allowlist` checks the
     union of the two lists (agent sessions are terminal sessions). Every other inline test
     keeps passing unchanged.
2. `src/process/mod.rs`: re-export `apply_confined_host_environment`,
   `apply_confined_host_environment_from` and `TERMINAL_ONLY_ENV_NAMES` beside the existing
   names. No other edit (the test-escape-policy stage runs concurrently and adds nothing here).
3. `src/handoff/completion/identity.rs`: `STAGE_ENVIRONMENT_POLICY` becomes
   `"stage-host-allowlist-v3"`. Fixtures that spell `"stage-host-allowlist-v2"` as a record
   value stay (the field is a free-form identity: `handoff/schema/completion.rs` validates only
   its syntax).
4. `src/fs/permissions/state_root.rs`: add `pub(crate) const LIVE_CREDENTIAL_FILE: &str =
   "live-credential";` with a doc line (the live profile's credential, set by `loom live
   credential set`; the host-job child reads it; no session and no confined command may) and
   append it to `STATE_ROOT_SECRET_FILES` (`[&str; 3]` becomes `[&str; 4]`). Update the module
   doc's list of secrets.
5. Tests that pin the exact state-root deny list gain the new entry beside
   `completion-attestation.key`: `src/plan_session/sandbox/tests.rs` and
   `src/plan_session/sandbox/tests_kinds_fixture.rs` (the `format!("/{}/...", state.display())`
   lists). `rg -n 'completion-attestation.key' src --glob '*tests*'` lists the candidates; a
   file that only checks membership needs no change. Report every file you changed.
6. `../loom-hooks/credential-guard.sh` rule (a): the `case "$TARGET_BASE"` line gains
   `live-credential` (`admin.token | user.token | completion-attestation.key |
   live-credential)`); the header comment's list of rule (a) names follows.
   `../loom-hooks/tests/credential-guard-tokens.sh`: one more case mirroring the
   `completion-attestation.key` one (a `live-credential` file under the main repo's state root,
   read through the worktree's symlink, blocked for `Read`).

## Traps

- `daemon/server/environment.rs` (`signing_variables_reach_the_daemon_child_but_never_a_stage_environment`)
  and `tests/session_auth_and_stalls_contracts.rs` call `apply_stage_environment_from`; they
  test terminal semantics and stay unchanged.
- Do not touch `AGENT_SESSION_ENV_NAMES` or `wrapper/script_text.rs`.
- Do not add the credential to `CREDENTIAL_FILE_DENY_READ_PATHS`: it is not a home file.
- `cargo fmt`; your one check: `cargo test --lib process::environment:: --no-run`.

## Report

Files changed, the confirmation that the concern was still open, every other caller of
`apply_stage_environment` you found that builds a confined (not a terminal) command, and every
test file whose expected list gained the new secret.
