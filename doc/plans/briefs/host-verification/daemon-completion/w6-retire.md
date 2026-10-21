# W6: retire the session-trusted and nested verification paths (stage daemon-completion)

Read `doc/plans/briefs/host-verification/common.md` first: sections 1, 4.4 and 4.6. After this stage no session runs a confined command. Everything that existed only to carry a session's verdict to the daemon, or to let a session nest confinement, goes. Paths are relative to `loom/`.

## Territory

You own:

- `src/commands/stage/completion_evidence.rs`, `src/commands/stage/completion_evidence/**`, `src/commands/stage/control_complete.rs`, `src/commands/stage/tests/control_complete.rs`, `src/commands/stage/mod.rs`;
- in `src/daemon/`: `mod.rs`, `inherited.rs`, `inherited/**`, `protocol.rs`, `protocol_debug.rs`;
- in `src/daemon/server/`: `mod.rs`, `client.rs`, `client_tests.rs`, `client_dispatch.rs`, `self_service.rs`, `self_service_tests.rs`, `tests/self_service_client.rs`, `completion_evidence.rs`, `completion_content_tests.rs`, `completion_dispatch.rs`, `completion_dispatch/**`, `control_complete.rs`, `control_complete_tests.rs`;
- `src/verify/criteria/confine_host.rs`, `src/orchestrator/terminal/native/session_settings/contents.rs`, `src/sandbox/settings/policy.rs`, `src/sandbox/settings/policy/tests.rs`, `src/fs/permissions/hooks/config.rs`, `src/codex.rs`;
- `tests/completion_replay.rs` and `tests/completion_replay/**`;
- `maintainability-baseline.txt`;
- `../loom-hooks/loom-control-complete.sh`, `../loom-hooks/codex-forward.sh`, `../loom-hooks/tests/loom-control-complete*.sh`, `../loom-hooks/tests/codex-forward-direct.sh`.

W4 removes every call into the broker from the completion command files it owns. W5 changes the freeze handlers. Never edit their files. Before deleting anything, run `loom map --impact <symbol>` and `rg -n <name> src tests ../loom-hooks`. A consumer outside your territory and outside W4's list (common.md and W4's brief) must be reported, not edited.

## Steps

1. **Completion broker and evidence protocol.**
   - Delete `src/commands/stage/completion_evidence.rs` and `completion_evidence/` (`broker.rs`, `producer.rs`, `output_criteria.rs`, `impact_record.rs`, and their tests).
   - Delete `src/commands/stage/control_complete.rs` and its test file.
   - Remove their declarations from `commands/stage/mod.rs`.
   - Before deleting `impact_record.rs`, check what besides the in-session printer uses it. The host-completion writer of `reviews/<stage>/impact-observed.json` (`record_observed`) must survive. If it lives in `impact_record.rs`, move it to a file under `src/verify/impact_tests/` and keep its tests.
2. **Daemon completion RPCs.**
   - Delete `daemon/server/completion_evidence.rs`, `completion_dispatch.rs` and `completion_dispatch/`, `control_complete.rs`, and their test files.
   - In `protocol.rs`, remove `Request::CompleteStage` and `Request::RecordCompletionEvidence` and every match arm on them (`client_dispatch.rs`, `self_service.rs`'s list, `protocol_debug.rs`), and the `AuthorizedCompletion` minting in `client.rs` if nothing else uses it.
   - Test helpers in the deleted files that other tests use (`TrustedCheckpointFixture`, `trusted_checkpoint_fixture`, `scratch_git_fixture`, `record_trusted_test_checkpoint`; find their users) move to a `#[cfg(test)]` support module beside their remaining users.
   - `observer.rs::check_completion_gates` stays: W4's apply calls it.
   - `user.token` itself stays; only its completion use goes. Check `rg -n 'user.token|completion_credential' src` and report any other user.
3. **macOS inherited confinement.**
   - Delete `daemon/inherited.rs` and `daemon/inherited/`, and `Request::ValidateInheritedConfinement` with its dispatch and self-service entry.
   - Delete the inherited branch in `verify/criteria/confine_host.rs`: the `inherited = network == Denied && daemon::inherited::available(&tree)` decision and the early return in `apply_environment`. Linux keeps bubblewrap and macOS keeps `sandbox-exec`, unconditionally.
   - In `session_settings/contents.rs`, delete the `#[cfg(target_os = "macos")]` block that adds the daemon socket to `sandbox.network.allowUnixSockets`, with its comment.
   - This code is `cfg(target_os = "macos")`, and nothing here compiles it. Delete whole items. Leave no dangling `use` under a macOS cfg. `rg -n 'target_os = "macos"' src` afterwards, and read every remaining hit near your edits.
4. **The opt-out.**
   - Delete the `allowNestedUserNamespaces` block and its comment from `sandbox/settings/policy.rs::sandbox_settings`.
   - In `sandbox/settings/policy/tests.rs`, replace `enabled_sandbox_allows_nested_user_namespaces_on_linux_only` with `enabled_sandbox_never_allows_nested_user_namespaces`. It asserts the key is absent for an enabled config on every platform.
5. **The completion hook.**
   - `../loom-hooks/loom-control-complete.sh`: delete the PostToolUse half (the broker call, output capture, outcome parsing and its messages). Keep the PreToolUse half: the pin of `loom stage complete <stage>` and the completion heartbeat (`loom_write_completion_heartbeat`). The stall monitor's `COMPLETION_ACTIVITY` depends on it.
   - In `src/fs/permissions/hooks/config.rs`, remove the hook's PostToolUse registration and keep its PreToolUse one.
   - Update or delete the `loom-control-complete*.sh` hook tests to match. A test that pinned broker behaviour goes.
   - `bash ../scripts/check-hook-syntax.sh` must stay green.
6. **Codex lane on Linux** (`../loom-hooks/codex-forward.sh`, `src/codex.rs` doc comment).
   - Today the wrapper probes `sandbox-exec` and falls back to direct mode (`codex exec --sandbox danger-full-access`) when the outer sandbox refuses a nested Seatbelt.
   - Add the Linux probe: when `uname -s` is `Linux` and `bwrap` is on PATH, run `bwrap --ro-bind / / --dev /dev true` (PATH lookup, so tests can stub it). When it fails, use direct mode with the existing note text adapted ("the outer sandbox refuses a nested bubblewrap").
   - Keep the macOS probe unchanged.
   - Extend `../loom-hooks/tests/codex-forward-direct.sh` with a Linux case: a stub `bwrap` that exits 1 → direct mode; one that exits 0 → companion mode. Mirror the existing `sandbox-exec` stub.
   - Update the `codex.rs` comment about the nested bubblewrap to say direct mode applies when nesting is refused.
7. **Delete `tests/completion_replay.rs` and `tests/completion_replay/`.** They drove the hook → broker → daemon path end to end. W5's `tests/host_verification_e2e.rs` replaces them.
8. **Maintainability ledger** (`maintainability-baseline.txt`).
   - Remove every row naming a file you deleted, or a function in one.
   - Never add a row, never raise one.
   - Run `cargo test --test maintainability` and fix what it reports within those rules. The main agent disputes the resulting `TI-ratchet` event.

## Proof (run once, from `loom/`, after W4 and W5 have written; if the build fails only on their missing items, report that and stop)

```bash
cargo build 2>&1 | tail -20
cargo test --lib sandbox::settings daemon:: verify::criteria::confine 2>&1 | tail -20
cd .. && bash loom-hooks/tests/run-all.sh 2>&1 | tail -15
rg -n 'allowNestedUserNamespaces|LOOM_CONTROL_EVIDENCE|ValidateInheritedConfinement|RecordCompletionEvidence|CompleteStage' loom/src loom-hooks
```

The last command must print nothing outside doc comments that W7 owns. Report any hit in a file you do not own.

## Report

Every file deleted; every symbol moved and where; consumers you found outside your territory; the macOS cfg sites you edited; the proof output.
